use gstreamer::glib;
use gstreamer::prelude::*;
use gstreamer::subclass::prelude::*;

glib::wrapper! {
    pub struct Duplicator(ObjectSubclass<imp::Duplicator>)
        @extends gstreamer::Element, gstreamer::Object;
}

mod imp {
    use gstreamer::{glib::derived_properties, FlowError, FlowSuccess};

    use super::*;
    use std::sync::{
        atomic::{AtomicU32, Ordering::Relaxed},
        LazyLock,
    };

    #[derive(glib::Properties)]
    #[properties(wrapper_type = super::Duplicator)]
    pub struct Duplicator {
        #[property(
            name = "copies",
            blurb = "Number of times each incoming buffer is forwarded downstream. Buffers are reffed (zero-copy). Set to 0 to drop all buffers.",
            default = 1,
            get,
            set,
            mutable_playing
        )]
        num_copies: AtomicU32,

        sinkpad: gstreamer::Pad,
        srcpad: gstreamer::Pad,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Duplicator {
        const NAME: &'static str = "RsDuplicator";
        type Type = super::Duplicator;
        type ParentType = gstreamer::Element;

        fn with_class(klass: &Self::Class) -> Self {
            let templ_sink = klass.pad_template("sink").unwrap();
            let templ_src = klass.pad_template("src").unwrap();

            let sinkpad = gstreamer::Pad::builder_from_template(&templ_sink)
                .chain_function(|pad, parent, buffer| {
                    Duplicator::catch_panic_pad_function(
                        parent,
                        || Err(FlowError::Error),
                        |this| this.sink_chain(pad, buffer),
                    )
                })
                .flags(gstreamer::PadFlags::PROXY_CAPS)
                .flags(gstreamer::PadFlags::PROXY_ALLOCATION)
                .build();

            let srcpad = gstreamer::Pad::builder_from_template(&templ_src)
                .flags(gstreamer::PadFlags::PROXY_CAPS)
                .flags(gstreamer::PadFlags::PROXY_ALLOCATION)
                .build();

            Self {
                num_copies: AtomicU32::new(1),
                sinkpad,
                srcpad,
            }
        }
    }

    #[derived_properties]
    impl ObjectImpl for Duplicator {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.add_pad(&self.sinkpad).unwrap();
            obj.add_pad(&self.srcpad).unwrap();
        }
    }

    impl GstObjectImpl for Duplicator {}

    impl ElementImpl for Duplicator {
        fn metadata() -> Option<&'static gstreamer::subclass::ElementMetadata> {
            static META: LazyLock<gstreamer::subclass::ElementMetadata> = LazyLock::new(|| {
                gstreamer::subclass::ElementMetadata::new(
                    "Buffer fan-out duplicator",
                    "Generic",
                    "Forwards each incoming buffer multiple times",
                    "Nikita Bloshchanevich <nikblos@outlook.com>",
                )
            });
            Some(&*META)
        }

        fn pad_templates() -> &'static [gstreamer::PadTemplate] {
            static TEMPLATES: LazyLock<Vec<gstreamer::PadTemplate>> = LazyLock::new(|| {
                let caps = gstreamer::Caps::new_any();

                let sink = gstreamer::PadTemplate::new(
                    "sink",
                    gstreamer::PadDirection::Sink,
                    gstreamer::PadPresence::Always,
                    &caps,
                )
                .unwrap();

                let src = gstreamer::PadTemplate::new(
                    "src",
                    gstreamer::PadDirection::Src,
                    gstreamer::PadPresence::Always,
                    &caps,
                )
                .unwrap();

                vec![sink, src]
            });

            TEMPLATES.as_ref()
        }
    }

    impl Duplicator {
        fn sink_chain(
            &self,
            _pad: &gstreamer::Pad,
            buffer: gstreamer::Buffer,
        ) -> Result<FlowSuccess, FlowError> {
            let num_copies = self.num_copies.load(Relaxed);
            for _ in 0..num_copies {
                let clone = buffer.clone();
                self.srcpad.push(clone)?;
            }

            Ok(FlowSuccess::Ok)
        }
    }
}
