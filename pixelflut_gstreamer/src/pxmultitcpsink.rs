use std::sync::LazyLock;

use gstreamer::glib;
use gstreamer_video::gst_base;

static CAT: LazyLock<gstreamer::DebugCategory> = LazyLock::new(|| {
    gstreamer::DebugCategory::new(
        "pxmultitcpsink",
        gstreamer::DebugColorFlags::empty(),
        Some("TCP Sink supporting multiple streams (esp. for Pixelflut)"),
    )
});

glib::wrapper! {
    pub struct PXMultiTCPSink(ObjectSubclass<imp::PXMultiTCPSink>) @extends gst_base::BaseSink;
}

mod imp {
    use super::CAT;
    use std::{
        cell::{Cell, OnceCell, RefCell},
        sync::{
            atomic::{AtomicI32, AtomicU32, AtomicUsize, Ordering::Relaxed},
            nonpoison::RwLock,
            LazyLock, Mutex,
        },
    };

    use gstreamer::subclass::prelude::*;
    use gstreamer::{
        glib, subclass::ElementMetadata, FlowError, FlowSuccess, PadDirection, PadPresence,
        PadTemplate,
    };
    use gstreamer::{prelude::*, StateChangeError};
    use gstreamer_video::gst_base::{self, subclass::prelude::BaseSinkImpl};
    use rayon::{
        iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator},
        ThreadPool, ThreadPoolBuilder,
    };

    use crate::{
        connectionpool::{ConnectionPool, ConnectionTuple},
        partitioned_buffer_meta::BufferPartitionMetadata,
        simplethreadpool::SimpleThreadPool,
    };

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::PXMultiTCPSink)]
    pub struct PXMultiTCPSink {
        pool: ConnectionPool,

        #[property(name = "hosts", get, set = Self::set_hosts, type = String, mutable_playing, blurb = "List of hosts to connect to. Syntax is <hostname or ip>:<port>(@<ip address>|/<netdev>)#multiplicity")]
        hosts: Mutex<String>,

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
    impl ObjectSubclass for PXMultiTCPSink {
        const NAME: &'static str = "pxmultitcpsink";

        type Type = super::PXMultiTCPSink;
        type ParentType = gst_base::BaseSink;
    }

    #[glib::derived_properties]
    impl ObjectImpl for PXMultiTCPSink {}

    impl GstObjectImpl for PXMultiTCPSink {}

    impl ElementImpl for PXMultiTCPSink {
        fn metadata() -> Option<&'static gstreamer::subclass::ElementMetadata> {
            static ELEMENT_DATA: LazyLock<ElementMetadata> = LazyLock::new(|| {
                ElementMetadata::new(
                    "Pixelflut Multi-stream TCP Sink",
                    "Sink",
                    "Sends a single buffer via multiple tcp sinks (load balancing)",
                    "Nikita Bloshchanevich <nikblos@outlook.com>",
                )
            });
            Some(&*ELEMENT_DATA)
        }

        fn pad_templates() -> &'static [gstreamer::PadTemplate] {
            static PAD_TEMPLATES: LazyLock<[PadTemplate; 1]> = LazyLock::new(|| {
                let sink = PadTemplate::new(
                    "sink",
                    PadDirection::Sink,
                    PadPresence::Always,
                    &gstreamer::Caps::new_any(),
                )
                .unwrap();
                [sink]
            });
            PAD_TEMPLATES.as_ref()
        }

        fn change_state(
            &self,
            transition: gstreamer::StateChange,
        ) -> Result<gstreamer::StateChangeSuccess, gstreamer::StateChangeError> {
            if transition.next() == gstreamer::State::Ready {
                self.threadpool.reconfigure(&self.nthreads, self, &CAT)?;
            }
            self.parent_change_state(transition)
        }
    }

    /// Properties
    impl PXMultiTCPSink {
        pub fn set_hosts(&self, hosts: String) {
            let Some(connections) = ConnectionTuple::parse(&hosts) else {
                gstreamer::error!(CAT, imp = self, "Syntax error in set hosts=`{hosts}`");
                return;
            };
            if let Err(e) = self.pool.set_connections(&connections) {
                gstreamer::error!(
                    CAT,
                    imp = self,
                    "set_connections() failed for hosts `{hosts}`: {e}"
                );
                return;
            }
            *self.hosts.lock().unwrap() = hosts;
        }
    }

    impl BaseSinkImpl for PXMultiTCPSink {
        fn render(
            &self,
            buffer: &gstreamer::Buffer,
        ) -> Result<gstreamer::FlowSuccess, gstreamer::FlowError> {
            if self.pool.is_empty() {
                gstreamer::error!(CAT, imp = self, "Must configure hosts= first!");
                return Err(FlowError::Error);
            }

            let buf_mem = buffer.map_readable().map_err(|e| {
                    gstreamer::error!(
                        CAT,
                        imp = self,
                        "Couldn't read-map buffer (NOTE: multiple GstMemory objects are not supported via BufferPartititionMetadata): {e}"
                    );
                    FlowError::Error
                })?;

            let handle_io_error = |e| {
                gstreamer::error!(CAT, imp = self, "Failed to write buffer (io-error): {e}");
                FlowError::Error
            };
            if let Some(partitions) = buffer.meta::<BufferPartitionMetadata>() {
                self.threadpool.install(|| {
                    partitions
                        .0
                        .meta
                        .bins
                        .par_iter()
                        .enumerate()
                        .for_each(|(robin_idx, bin)| {
                            let data = &buf_mem[bin.offset..bin.offset + bin.len];
                            let _ = self.pool.send(robin_idx, data).map_err(handle_io_error);
                        });
                });
            } else {
                self.pool.send(0, &buf_mem).map_err(handle_io_error)?;
            }

            Ok(FlowSuccess::Ok)
        }
    }
}
