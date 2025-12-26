use std::{fmt::write, ptr};

use crate::{
    base::Coord,
    encoding_ng::{
        encode_offset_command, encode_px_command_lite_gray, encode_px_command_lite_rgb,
        encode_px_command_lite_rgba,
    },
};

#[derive(Clone, Copy)]
pub enum ImageFormat {
    Rgba,
    Rgb,
    Rgbx,
    Gray,
}

#[derive(Clone)]
pub struct ImageMetadata {
    pub stride: isize,
    pub image_format: ImageFormat,
    pub width: Coord,
    pub height: Coord,
}

pub struct ImageData {
    pub pixels: *const u8,
    pub meta: ImageMetadata,
}

impl ImageData {
    pub fn pixel_stride(&self) -> usize {
        match self.meta.image_format {
            ImageFormat::Rgba => 4,
            ImageFormat::Rgb => 3,
            ImageFormat::Rgbx => 4,
            ImageFormat::Gray => 1,
        }
    }
}

pub struct EncodeSettings {
    pub x_base: Coord,
    pub y_base: Coord,
}

pub fn calc_pixelflut_frame_size(width: Coord, height: Coord, image_format: ImageFormat) -> usize {
    let width = width as u32;
    let height = height as u32;
    let n_chunks = (width.div_ceil(10) * height.div_ceil(10)) as usize;
    let offset_commands = n_chunks * b"OFFSET 1234 1234\n".len();
    let pixel_data_len = match image_format {
        ImageFormat::Rgba => b"rrggbbaa".len(),
        ImageFormat::Rgb | ImageFormat::Rgbx => b"rrggbb".len(),
        ImageFormat::Gray => b"gg".len(),
    };
    let px_commands = (width.next_multiple_of(10) * height.next_multiple_of(10)) as usize
        * (b"PX 1 1 \n".len() + pixel_data_len);
    let slack = 16;
    offset_commands + px_commands + slack
}

pub unsafe fn encode_image(image: &ImageData, out: &mut [u8], settings: &EncodeSettings) -> usize {
    assert!(out.len() >= calc_pixelflut_frame_size(image.meta.width, image.meta.height, ImageFormat::Rgba));

    let mut writeptr = out.as_mut_ptr();
    let mut imagedataptr_strided = image.pixels;

    for y_chunk in 0..image.meta.height / 10 {
        let mut imagedataptr = imagedataptr_strided;
        for x_chunk in 0..image.meta.width / 10 {
            unsafe {
                let len = encode_offset_command(
                    x_chunk + settings.x_base,
                    y_chunk + settings.y_base,
                    writeptr,
                );
                writeptr = writeptr.add(len);
            }

            for dy in 0..9 {
                for dx in 0..9 {
                    unsafe {
                        let wslice = &mut *(writeptr as *mut _);
                        let write_len: usize = match image.meta.image_format {
                            ImageFormat::Rgba => {
                                let pixel = ptr::read_unaligned(imagedataptr as *const u32);
                                encode_px_command_lite_rgba(dx, dy, pixel, wslice).into()
                            }
                            ImageFormat::Rgb => {
                                let pixel = ptr::read_unaligned(imagedataptr as *const u32);
                                encode_px_command_lite_rgb(dx, dy, pixel, wslice).into()
                            }
                            ImageFormat::Rgbx => {
                                let pixel = ptr::read_unaligned(imagedataptr as *const u32);
                                encode_px_command_lite_rgb(dx, dy, pixel, wslice).into()
                            }
                            ImageFormat::Gray => {
                                let pixel = ptr::read_unaligned(imagedataptr as *const u8);
                                encode_px_command_lite_gray(dx, dy, pixel, wslice).into()
                            }
                        };
                        writeptr = writeptr.byte_add(write_len);
                        imagedataptr = imagedataptr.byte_add(image.pixel_stride());
                    }
                }
            }
        }

        unsafe {
            imagedataptr_strided = imagedataptr_strided.byte_offset(image.meta.stride);
        }
    }

    unsafe {
        let total_len = writeptr.offset_from_unsigned(out.as_ptr());
        total_len
    }
}

#[cfg(test)]
mod benches {
    use std::hint::black_box;

    use super::*;
    use crate::blit_image_ng::{calc_pixelflut_frame_size, encode_image, ImageData};
    use test::Bencher;

    #[bench]
    pub fn bench_encode_image(bench: &mut Bencher) {
        let width = 1920;
        let height = 1080;
        let mut image_data = Vec::new();
        image_data.resize(1920 * 1080, 0xFFu32);
        let mut out_block = Vec::new();
        out_block.resize(
            calc_pixelflut_frame_size(width, height, ImageFormat::Rgba),
            0xFu8,
        );

        bench.iter(|| {
            let image = black_box(ImageData {
                pixels: image_data.as_slice().as_ptr() as *const _,
                meta: ImageMetadata { 
                    stride: 1080 * 4,
                    image_format: ImageFormat::Rgbx,
                    width,
                    height,
                }
            });
            let settings = black_box(EncodeSettings {
                x_base: 0,
                y_base: 0,
            });
            unsafe {
                let len = black_box(encode_image(
                    black_box(&image),
                    black_box(&mut out_block),
                    black_box(&settings),
                ));
                assert!(len >= calc_pixelflut_frame_size(width, height, ImageFormat::Rgba) / 2 /* we should be *at least* within 1/2 of the estimate, else something was very wrong with writing */);
            }
            black_box(&image_data);
            black_box(&out_block);
        });
    }
}
