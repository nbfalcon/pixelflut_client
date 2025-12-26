use std::sync::LazyLock;

use gstreamer::{
    glib::{self, types::StaticType},
    Rank,
};
use gstreamer_video::gst_base;

static CAT: LazyLock<gstreamer::DebugCategory> = LazyLock::new(|| {
    gstreamer::DebugCategory::new(
        "pixeltflut_gstreamer",
        gstreamer::DebugColorFlags::empty(),
        Some("Pixelflut Gstreamer"),
    )
});

mod imp {
    use gstreamer::{
        glib::{
            self,
            subclass::{object::ObjectImpl, types::ObjectSubclass},
            value::ToValue,
            ParamSpecBuilderExt,
        },
        prelude::GstParamSpecBuilderExt,
        subclass::{
            prelude::{ElementImpl, GstObjectImpl},
            ElementMetadata,
        },
        FlowError, FlowSuccess, PadDirection, PadPresence, PadTemplate, Structure,
    };
    use gstreamer_video::{
        gst_base::{
            self,
            subclass::{prelude::BaseTransformImpl, BaseTransformMode},
        },
        VideoFormat,
    };
    use pixelflut_base::blit_image_ng::{
        encode_image, EncodeSettings, ImageData, ImageFormat, ImageMetadata,
    };
    use pixelflut_base::{base::*, pixelflut_builder::PixelflutBuilder};
    use std::sync::{
        atomic::{AtomicU16, Ordering},
        LazyLock, Mutex,
    };

    use crate::pixelflut_gstreamer::CAT;

    #[derive(Default)]
    pub struct PixelflutConvert {
        // FIXME: ImageInfo is POD, we can use non-Mutex
        image: Mutex<Option<ImageMetadata>>,

        offset_x: AtomicU16,
        offset_y: AtomicU16,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PixelflutConvert {
        const NAME: &'static str = "rsimage2pixelflut";

        type Type = super::PixelflutConvert;
        type ParentType = gst_base::BaseTransform;
    }

    impl ObjectImpl for PixelflutConvert {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: LazyLock<[glib::ParamSpec; 2]> = LazyLock::new(|| {
                [
                    glib::ParamSpecUInt::builder("offset-x")
                        .nick("X")
                        .blurb("X of top-left corner for pixels")
                        .default_value(0)
                        .minimum(0)
                        .maximum(Coord::MAX as u32)
                        .mutable_playing()
                        .build(),
                    glib::ParamSpecUInt::builder("offset-y")
                        .nick("Y")
                        .blurb("Y of top-left corner for pixels")
                        .default_value(0)
                        .minimum(0)
                        .maximum(Coord::MAX as u32)
                        .mutable_playing()
                        .build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "offset-x" => (self.offset_x.load(Ordering::Relaxed) as u32).to_value(),
                "offset-y" => (self.offset_y.load(Ordering::Relaxed) as u32).to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                name @ ("offset-x" | "offset-y") => {
                    let v = value.get::<u32>().unwrap().try_into().unwrap();
                    let d = if name == "offset-x" {
                        &self.offset_x
                    } else {
                        &self.offset_y
                    };
                    d.store(v, Ordering::Relaxed);
                }
                _ => unimplemented!(),
            }
        }
    }

    impl GstObjectImpl for PixelflutConvert {}

    impl BaseTransformImpl for PixelflutConvert {
        const MODE: gst_base::subclass::BaseTransformMode = BaseTransformMode::NeverInPlace;

        const PASSTHROUGH_ON_SAME_CAPS: bool = false;
        const TRANSFORM_IP_ON_PASSTHROUGH: bool = false;

        fn transform(
            &self,
            inbuf: &gstreamer::Buffer,
            outbuf: &mut gstreamer::BufferRef,
        ) -> Result<FlowSuccess, gstreamer::FlowError> {
            let mapped_in = inbuf.map_readable().map_err(|_| FlowError::Error)?;

            let len = {
                let mut mapped_out = outbuf.map_writable().map_err(|_| FlowError::Error)?;
                let Some(image_info) = self.image.lock().unwrap().clone() else {
                    return Err(gstreamer::FlowError::NotNegotiated);
                };

                let image = ImageData {
                    pixels: mapped_in.as_ptr(),
                    meta: image_info,
                };
                let settings = EncodeSettings {
                    x_base: self.offset_x.load(Ordering::Relaxed),
                    y_base: self.offset_y.load(Ordering::Relaxed),
                };

                unsafe { encode_image(&image, &mut mapped_out, &settings) }
            };
            outbuf.set_size(len);

            Ok(FlowSuccess::Ok)
        }

        fn set_caps(
            &self,
            incaps: &gstreamer::Caps,
            _outcaps: &gstreamer::Caps,
        ) -> Result<(), gstreamer::LoggableError> {
            let image_info = image_meta_from_caps(incaps)?;
            *self.image.lock().unwrap() = Some(image_info);

            Ok(())
        }

        fn transform_size(
            &self,
            _direction: gstreamer::PadDirection,
            caps: &gstreamer::Caps,
            _size: usize,
            _othercaps: &gstreamer::Caps,
        ) -> Option<usize> {
            image_meta_from_caps(caps).ok().map(|image_info| {
                PixelflutBuilder::required_size(image_info.width, image_info.height)
            })
        }
    }

    fn image_meta_from_caps(
        incaps: &gstreamer::Caps,
    ) -> Result<ImageMetadata, gstreamer::LoggableError> {
        let videoinfo = gstreamer_video::VideoInfo::from_caps(incaps)?;
        let image_format = match videoinfo.format() {
            VideoFormat::Rgba => ImageFormat::Rgba,
            VideoFormat::Rgb => ImageFormat::Rgb,
            VideoFormat::Rgbx => ImageFormat::Rgbx,
            VideoFormat::Gray8 => ImageFormat::Gray,
            invalid => {
                return Err(gstreamer::LoggableError::new(
                    *CAT,
                    glib::bool_error!("Only {{Rgba,Rgb,Rgbx,Gray8}} is supported, got {invalid}"),
                ))
            }
        };
        let width = videoinfo.width();
        let height = videoinfo.height();
        if width % 10 != 0 || height != 0 {
            return Err(gstreamer::LoggableError::new(
                *CAT,
                glib::bool_error!(
                    "width/height must currently be divisible by 10; got w={width},h={height}"
                ),
            ));
        }
        let stride = videoinfo.stride()[0];

        Ok(ImageMetadata {
            stride: stride as isize,
            image_format,
            width: width as Coord,
            height: height as Coord,
        })
    }

    impl ElementImpl for PixelflutConvert {
        fn metadata() -> Option<&'static gstreamer::subclass::ElementMetadata> {
            static ELEMENT_DATA: LazyLock<ElementMetadata> = LazyLock::new(|| {
                ElementMetadata::new(
                    "Image2Pixelflut Converter",
                    "Filter",
                    "Converts images to Pixelflut TCP",
                    "Nikita Bloshchanevich <nikblos@outlook.com>",
                )
            });
            Some(&*ELEMENT_DATA)
        }

        fn pad_templates() -> &'static [gstreamer::PadTemplate] {
            static PAD_TEMPLATES: LazyLock<[PadTemplate; 2]> = LazyLock::new(|| {
                let sink = PadTemplate::new(
                    "sink",
                    PadDirection::Sink,
                    PadPresence::Always,
                    &gstreamer::Caps::builder_full()
                        .structure(
                            Structure::builder("video/x-raw")
                                .field(
                                    "format",
                                    gstreamer::List::from_values([
                                        VideoFormat::Rgba.to_str().into(),
                                        VideoFormat::Rgb.to_str().into(),
                                        VideoFormat::Rgbx.to_str().into(),
                                        VideoFormat::Gray8.to_str().into(),
                                    ]),
                                )
                                .field("width", gstreamer::IntRange::new(1, 9999))
                                .field("height", gstreamer::IntRange::new(1, 9999))
                                .build(),
                        )
                        .build(),
                )
                .unwrap();
                let src = PadTemplate::new(
                    "src",
                    PadDirection::Src,
                    PadPresence::Always,
                    &gstreamer::Caps::new_any(),
                )
                .unwrap();

                [sink, src]
            });

            &*PAD_TEMPLATES
        }
    }
}

glib::wrapper! {
    pub struct PixelflutConvert(ObjectSubclass<imp::PixelflutConvert>) @extends gst_base::BaseTransform;
}

pub fn plugin_init(plugin: &gstreamer::Plugin) -> Result<(), glib::BoolError> {
    gstreamer::Element::register(
        Some(plugin),
        "rsimage2pixelflut",
        Rank::NONE,
        PixelflutConvert::static_type(),
    )?;
    Ok(())
}
