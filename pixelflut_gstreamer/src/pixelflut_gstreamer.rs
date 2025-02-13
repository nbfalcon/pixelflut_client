use gstreamer::{
    glib::{self, types::StaticType},
    Rank,
};
use gstreamer_video::gst_base;

mod imp {
    use crate::{
        blit_image::{blit_image, ImageInfo},
        pixelflut_builder::PixelflutBuilder,
    };
    use gstreamer::{
        glib::{
            self,
            subclass::{object::ObjectImpl, types::ObjectSubclass},
        },
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
    use std::sync::{LazyLock, Mutex};

    #[derive(Default)]
    pub struct PixelflutConvert {
        // FIXME: ImageInfo is POD, we can use non-Mutex
        image: Mutex<ImageInfo>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PixelflutConvert {
        const NAME: &'static str = "rsimage2pixelflut";

        type Type = super::PixelflutConvert;
        type ParentType = gst_base::BaseTransform;
    }

    impl ObjectImpl for PixelflutConvert {}
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
                let image_info = self.image.lock().unwrap().clone();
                let mut writer = PixelflutBuilder::with_xy_capacity(
                    &mut mapped_out,
                    image_info.width,
                    image_info.height,
                );
                blit_image(&mut writer, &mapped_in, &image_info);
                writer.as_slice().len()
            };
            outbuf.set_size(len);

            Ok(FlowSuccess::Ok)
        }

        fn set_caps(
            &self,
            incaps: &gstreamer::Caps,
            _outcaps: &gstreamer::Caps,
        ) -> Result<(), gstreamer::LoggableError> {
            let image_info = image_info_from_caps(incaps)?;
            *self.image.lock().unwrap() = image_info;

            Ok(())
        }

        fn transform_size(
            &self,
            _direction: gstreamer::PadDirection,
            caps: &gstreamer::Caps,
            _size: usize,
            _othercaps: &gstreamer::Caps,
        ) -> Option<usize> {
            image_info_from_caps(caps).ok().map(|image_info| {
                PixelflutBuilder::required_size(image_info.width, image_info.height)
            })
        }
    }

    fn image_info_from_caps(
        incaps: &gstreamer::Caps,
    ) -> Result<ImageInfo, gstreamer::LoggableError> {
        let videoinfo = gstreamer_video::VideoInfo::from_caps(incaps)?;
        let stride = videoinfo.stride()[0];
        let stride: u32 = stride.try_into().expect("BUG: Negative stride???");
        let stride_extra = stride
            .checked_sub(videoinfo.width() * 4)
            .expect("BUG: Stride < width???");
        let image_info = ImageInfo {
            width: videoinfo.width() as u16,
            height: videoinfo.height() as u16,
            stride_extra,
        };
        Ok(image_info)
    }

    impl ElementImpl for PixelflutConvert {
        fn metadata() -> Option<&'static gstreamer::subclass::ElementMetadata> {
            static ELEMENT_DATA: LazyLock<ElementMetadata> = LazyLock::new(|| {
                ElementMetadata::new(
                    "Image2Pixelflut Convert",
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
                                .field("format", VideoFormat::Rgba.to_str())
                                .field("width", gstreamer::IntRange::new(1, 65536 - 1))
                                .field("height", gstreamer::IntRange::new(1, 65536 - 1))
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
