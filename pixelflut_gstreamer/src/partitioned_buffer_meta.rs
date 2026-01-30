use std::mem::ManuallyDrop;

use gstreamer::MetaAPIExt;

/// Buffer metadata for "partitioned" streaming buffers - to leverage multiple streams.
/// The pixelflut GstBaseTransform will take an image, and translate it into multiple byte sequences
/// that cann all be written independently. Since there is only a single GstBuffer,
/// downstream needs to know which parts are "independent" (so that a PX command isn't cut in the middle)
/// This is done by the metadata here: each bin has an (offset, len), specifying the part of the buffer
/// it should write out.

#[derive(Clone)]
pub struct BufferPartitionBin {
    pub offset: usize,
    pub len: usize,
}

/// Indicates that the GstBuffer (containg raw bytes) is "partitioned", into "Bins" that can be written in parallel/independently.
#[derive(Clone)]
pub struct BufferPartitionMetadataParams {
    pub bins: Vec<BufferPartitionBin>,
}

#[repr(transparent)]
pub struct BufferPartitionMetadata(pub imp::GstBufferPartitionMetadata);
unsafe impl Send for BufferPartitionMetadata {}
unsafe impl Sync for BufferPartitionMetadata {}

unsafe impl gstreamer::MetaAPI for BufferPartitionMetadata {
    type GstType = imp::GstBufferPartitionMetadata;

    fn meta_api() -> gstreamer::glib::Type {
        imp::custom_meta_api_get_type()
    }
}

pub fn attach_to_buffer(
    buffer: &mut gstreamer::BufferRef,
    meta: BufferPartitionMetadataParams,
) -> gstreamer::MetaRefMut<'_, BufferPartitionMetadata, gstreamer::meta::Standalone> {
    let mut meta_params = ManuallyDrop::new(meta);

    unsafe {
        let meta_response = gstreamer::ffi::gst_buffer_add_meta(
            buffer.as_mut_ptr(),
            imp::custom_meta_get_info(),
            &mut meta_params as *mut _ as *mut _,
        ) as *mut imp::GstBufferPartitionMetadata;

        BufferPartitionMetadata::from_mut_ptr(buffer, meta_response)
    }

    // Do not drop, custom_meta_init will already have done so.
}

pub(crate) mod imp {
    use core::ptr;
    use std::mem;

    use gstreamer::glib::{
        self,
        translate::{FromGlib, IntoGlib},
    };

    use crate::partitioned_buffer_meta::{attach_to_buffer, BufferPartitionMetadataParams};

    #[repr(C)]
    pub struct GstBufferPartitionMetadata {
        parent: gstreamer::ffi::GstMeta,
        pub meta: BufferPartitionMetadataParams,
    }

    // Function to register the meta API and get a type back.
    pub(super) fn custom_meta_api_get_type() -> glib::Type {
        static TYPE: std::sync::OnceLock<glib::Type> = std::sync::OnceLock::new();
        *TYPE.get_or_init(|| unsafe {
            let t = glib::Type::from_glib(gstreamer::ffi::gst_meta_api_type_register(
                c"BufferPartitionMetadataAPI".as_ptr() as *const _,
                // We provide no tags here as our meta is just a label and does
                // not refer to any specific aspect of the buffer.
                [ptr::null::<std::os::raw::c_char>()].as_ptr() as *mut *const _,
            ));
            assert_ne!(t, glib::Type::INVALID);
            t
        })
    }

    // Initialization function for our meta. This needs to ensure all fields are correctly
    // initialized. They will contain random memory before.
    unsafe extern "C" fn custom_meta_init(
        meta: *mut gstreamer::ffi::GstMeta,
        params: glib::ffi::gpointer,
        _buffer: *mut gstreamer::ffi::GstBuffer,
    ) -> glib::ffi::gboolean {
        assert!(!params.is_null());

        // This might not be fully initialized
        let meta = meta as *mut GstBufferPartitionMetadata;
        let meta_ours = &raw mut (*meta).meta;

        // This is fully initalized; caller will not drop it.
        let params = ptr::read(params as *const BufferPartitionMetadataParams);
        unsafe {
            ptr::write(meta_ours, params);
        }

        true.into_glib()
    }

    // Free function for our meta. This needs to free/drop all memory we allocated.
    unsafe extern "C" fn custom_meta_free(
        meta: *mut gstreamer::ffi::GstMeta,
        _buffer: *mut gstreamer::ffi::GstBuffer,
    ) {
        let meta = &raw mut (*(meta as *mut GstBufferPartitionMetadata)).meta;

        // Need to free/drop all our fields here.
        ptr::drop_in_place(meta);
    }

    // Transform function for our meta. This needs to get it from the old buffer to the new one
    // in a way that is compatible with the transformation type. In this case we just always
    // copy it over.
    unsafe extern "C" fn custom_meta_transform(
        dest: *mut gstreamer::ffi::GstBuffer,
        meta: *mut gstreamer::ffi::GstMeta,
        _buffer: *mut gstreamer::ffi::GstBuffer,
        _type_: glib::ffi::GQuark,
        _data: glib::ffi::gpointer,
    ) -> glib::ffi::gboolean {
        let meta = &*(meta as *const GstBufferPartitionMetadata);

        attach_to_buffer(gstreamer::BufferRef::from_mut_ptr(dest), meta.meta.clone());

        true.into_glib()
    }

    // Register the meta itself with its functions.
    pub(super) fn custom_meta_get_info() -> *const gstreamer::ffi::GstMetaInfo {
        struct MetaInfo(ptr::NonNull<gstreamer::ffi::GstMetaInfo>);
        unsafe impl Send for MetaInfo {}
        unsafe impl Sync for MetaInfo {}

        static META_INFO: std::sync::OnceLock<MetaInfo> = std::sync::OnceLock::new();

        META_INFO
            .get_or_init(|| unsafe {
                MetaInfo(
                    ptr::NonNull::new(gstreamer::ffi::gst_meta_register(
                        custom_meta_api_get_type().into_glib(),
                        c"BufferPartitionMetadata".as_ptr() as *const _,
                        mem::size_of::<GstBufferPartitionMetadata>(),
                        Some(custom_meta_init),
                        Some(custom_meta_free),
                        Some(custom_meta_transform),
                    ) as *mut gstreamer::ffi::GstMetaInfo)
                    .expect("Failed to register meta API"),
                )
            })
            .0
            .as_ptr()
    }
}
