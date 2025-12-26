use core::ptr;
use crate::base::*;
use crate::pixelflut_builder::{Color, PixelflutBuilder};

pub enum ImageFormat {
    Rgb,
    Rgba,
}

#[derive(Default, Clone, Copy)]
pub struct ImageInfo {
    pub width: Coord,
    pub height: Coord,

    pub stride: isize,
}

pub fn blit_image(
    out: &mut PixelflutBuilder,
    image_data: &[u8],
    image_info: &ImageInfo,
    offset_x: Coord,
    offset_y: Coord,
) {
    assert!(out.check_capacity((image_info.height as usize) * (image_info.width as usize)));
    // println!("{}/{}", image_data.len(), image_info.stride_extra);
    assert!(
        (image_info.width as usize + image_info.stride as usize)
            * (image_info.height as usize)
            * 4
            - image_info.stride as usize
            <= image_data.len()
    );

    for y in 0..image_info.height {
        for x in 0..image_info.width {
            let idx = (image_info.width as isize + image_info.stride as isize) * (y as isize)
                + (x as isize);
            let idxpx = idx * 4;

            let px_data = unsafe {
                let ptr = image_data.as_ptr().offset(idxpx) as *const u32;
                ptr::read_unaligned(ptr)
            };
            let px: Color = unsafe { std::mem::transmute(px_data) };
            out.cmd_px(x.wrapping_add(offset_x), y.wrapping_add(offset_y), px);
        }
    }
}
