use std::sync::LazyLock;

use gstreamer::glib;
use gstreamer_video::gst_base;

static CAT: LazyLock<gstreamer::DebugCategory> = LazyLock::new(|| {
    gstreamer::DebugCategory::new(
        "pixeltflut_gstreamer",
        gstreamer::DebugColorFlags::empty(),
        Some("Pixelflut Gstreamer"),
    )
});

mod imp {
    use super::CAT;
    use crate::partitioned_buffer_meta::{
        attach_to_buffer, BufferPartitionBin, BufferPartitionMetadataParams,
    };
    use crate::simplethreadpool::SimpleThreadPool;
    use gstreamer::subclass::prelude::*;
    use gstreamer::{
        glib::{
            self,
            property::{self, PropertyGet},
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
    use gstreamer::{prelude::*, Buffer, State};
    use gstreamer_video::gst_base::prelude::BaseTransformExt;
    use gstreamer_video::gst_base::subclass::prelude::BaseTransformImplExt;
    use gstreamer_video::{
        gst_base::{
            self,
            subclass::{prelude::BaseTransformImpl, BaseTransformMode},
        },
        VideoFormat,
    };
    use pixelflut_base::base::*;
    use pixelflut_base::blit_image_ng::{
        calc_pixelflut_frame_size, calc_pixelflut_image_meta_size, encode_image, EncodeSettings,
        ImageData, ImageFormat, ImageMetadata,
    };
    use rayon::prelude::*;
    use std::cmp::min;
    use std::sync::atomic::Ordering::Relaxed;
    use std::sync::atomic::{AtomicI32, AtomicU32};
    use std::sync::{LazyLock, Mutex};

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::PixelflutConvert)]
    pub struct PixelflutConvert {
        // FIXME: ImageInfo is POD, we can use non-Mutex
        image: Mutex<Option<ImageMetadata>>,

        #[property(name = "offset-x", nick ="X", blurb = "X of top-left corner for pixels", minimum = 0, maximum = Coord::MAX as u32, get, set, mutable_playing)]
        offset_x: AtomicU32,
        #[property(name = "offset-y", nick ="Y", blurb = "Y of top-left corner for pixels", minimum = 0, maximum = Coord::MAX as u32, get, set, mutable_playing)]
        offset_y: AtomicU32,

        // The npartitions value as used for the buffer that will be transform()'d next.
        current_buffer_npartitions: AtomicU32,
        #[property(
            name = "partitions",
            blurb = "If not zero, enable encoding using multiple threads + BufferPartitionMetadata. This only works with pxmultitcpsink! In that case, specifies the number of thredas.",
            get,
            set,
            mutable_playing
        )]
        npartitions: AtomicU32,

        #[property(
            name = "threads",
            get,
            set,
            blurb = "Number of threads to use for sending (0: use the global rayon pool)"
        )]
        nthreads: AtomicI32,
        threadpool: SimpleThreadPool,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PixelflutConvert {
        const NAME: &'static str = "rsimage2pixelflut";

        type Type = super::PixelflutConvert;
        type ParentType = gst_base::BaseTransform;
    }

    #[glib::derived_properties]
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

            let x_base = self.offset_x.load(Relaxed) as u16;
            let y_base = self.offset_y.load(Relaxed) as u16;
            let npartitions = self.current_buffer_npartitions.load(Relaxed) as u16;

            let mut mapped_out = outbuf.map_writable().map_err(|_| FlowError::Error)?;
            let Some(image_info) = self.image.lock().unwrap().clone() else {
                return Err(gstreamer::FlowError::NotNegotiated);
            };
            let image: ImageData = ImageData {
                pixels: mapped_in.as_ptr(),
                meta: image_info,
            };

            if npartitions == 0 {
                let settings = EncodeSettings { x_base, y_base };
                let len = unsafe { encode_image(&image, &mut mapped_out, &settings) };
                drop(mapped_out);
                outbuf.set_size(len);
            } else {
                let height_to_outbuf_size = |height| {
                    calc_pixelflut_frame_size(image_info.width, height, image_info.image_format)
                };
                let fill_row_range = |start_y, end_y, outbuf_slice: &mut [u8]| {
                    let image_slice = image.slice_rows(start_y, end_y);
                    let settings_slice = EncodeSettings {
                        x_base,
                        y_base: y_base + start_y,
                    };
                    unsafe { encode_image(&image_slice, outbuf_slice, &settings_slice) }
                };
                let partitions = self.threadpool.install(|| {
                    partition_via_rows(
                        image_info.height,
                        npartitions as u16,
                        &mut mapped_out,
                        height_to_outbuf_size,
                        fill_row_range,
                    )
                });
                drop(mapped_out);
                outbuf.set_size(outbuf.maxsize());
                attach_to_buffer(outbuf, BufferPartitionMetadataParams { bins: partitions });
            }

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
            let npartitions: u16 = self.npartitions.load(Relaxed).try_into().unwrap();
            self.current_buffer_npartitions
                .store(npartitions as u32, Relaxed);
            let meta = image_meta_from_caps(caps).ok()?;
            Some(if npartitions == 0 {
                calc_pixelflut_image_meta_size(meta)
            } else {
                let row_skip = calculate_row_skip(meta.height, npartitions);
                npartitions as usize
                    * calc_pixelflut_frame_size(meta.width, row_skip, meta.image_format)
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
        if width % 10 != 0 || height % 10 != 0 {
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

    fn partition_via_rows(
        height: Coord,
        npartitions: Coord,
        outbuf: &mut [u8],
        calculate_size: impl Fn(Coord) -> usize,
        callback: impl Fn(Coord, Coord, &mut [u8]) -> usize + Send + Sync,
    ) -> Vec<BufferPartitionBin> {
        let row_skip = calculate_row_skip(height, npartitions);
        let outbuf_skip = calculate_size(row_skip);
        assert!(outbuf.len() >= outbuf_skip * npartitions as usize);

        let mut result = Vec::with_capacity(npartitions as usize);
        (0..height)
            .into_par_iter()
            .step_by(row_skip as usize)
            .enumerate()
            .zip(outbuf.par_chunks_exact_mut(outbuf_skip))
            .map(|((i, start_row), outbuf_slice)| {
                let end_row = min(start_row + row_skip, height);
                let start_outbuf = outbuf_skip * i;

                let len = callback(start_row, end_row, outbuf_slice);
                BufferPartitionBin {
                    offset: start_outbuf,
                    len,
                }
            })
            .collect_into_vec(&mut result);
        result
    }

    fn calculate_row_skip(height: u16, npartitions: u16) -> u16 {
        height.div_ceil(npartitions).next_multiple_of(10)
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
                                // Due to the 10x10 calculation, we can't go lower than step 10
                                .field("width", gstreamer::IntRange::with_step(10, 9990, 10))
                                .field("height", gstreamer::IntRange::with_step(10, 9990, 10))
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

        fn change_state(
            &self,
            transition: gstreamer::StateChange,
        ) -> Result<gstreamer::StateChangeSuccess, gstreamer::StateChangeError> {
            if transition.next() == State::Ready {
                self.threadpool.reconfigure(&self.nthreads, self, &CAT)?;
            }
            self.parent_change_state(transition)
        }
    }
}

glib::wrapper! {
    pub struct PixelflutConvert(ObjectSubclass<imp::PixelflutConvert>) @extends gst_base::BaseTransform;
}
