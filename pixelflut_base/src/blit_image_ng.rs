use crate::{
    base::Coord,
    encoding_ng::{encode_offset_command, encode_px_command_lite_rgba},
};

pub struct ImageData<'a> {
    pixel_data: &'a [u32],
    width: Coord,
    height: Coord,
}

fn calc_pixelflut_frame_size(width: Coord, height: Coord) -> usize {
    let width = width as u32;
    let height = height as u32;
    let n_chunks = (width.div_ceil(10) * height.div_ceil(10)) as usize;
    let offset_commands = n_chunks * b"OFFSET 1234 1234\n".len();
    let px_commands = (width.next_multiple_of(10) * height.next_multiple_of(10)) as usize
        * b"PX 1 1 rrggbbaa\n".len();
    let slack = 128;
    offset_commands + px_commands + slack
}

pub fn encode_image(image: ImageData, out: &mut [u8]) -> usize {
    assert!(out.len() >= calc_pixelflut_frame_size(image.width, image.height));
    assert!(image.pixel_data.len() >= image.width as usize * image.height as usize);

    let width: Coord = image.width;
    let height: Coord = image.height;
    let x_base: Coord = 0;
    let y_base: Coord = 0;

    let write_startptr = out.as_mut_ptr();
    let mut writeptr = write_startptr;
    let mut imagedataptr = 0;

    for y_chunk in 0..height / 10 {
        for x_chunk in 0..width / 10 {
            unsafe {
                let len = encode_offset_command(x_chunk + x_base, y_chunk + y_base, writeptr);
                writeptr = writeptr.add(len);
            }

            for dy in 0..9 {
                for dx in 0..9 {
                    let len = encode_px_command_lite_rgba(
                        dx,
                        dy,
                        image.pixel_data[imagedataptr],
                        unsafe { &mut *(writeptr as *mut _) },
                    );
                    imagedataptr += 1;
                    unsafe {
                        writeptr = writeptr.add(len.into());
                    }
                }
            }
        }
    }

    unsafe {
        let total_len = writeptr.offset_from_unsigned(write_startptr);
        total_len
    }
}

#[cfg(test)]
mod benches {
    use std::hint::black_box;

    use test::Bencher;

    use crate::blit_image_ng::{calc_pixelflut_frame_size, encode_image};

    #[bench]
    pub fn bench_encode_image(bench: &mut Bencher) {
        let width = 1920;
        let height = 1080;
        let mut image_data = Vec::new();
        image_data.resize(1920 * 1080, 0xFFu32);
        let mut out_block = Vec::new();
        out_block.resize(calc_pixelflut_frame_size(width, height), 0xFu8);

        bench.iter(|| {
            let len = encode_image(
                black_box(super::ImageData {
                    pixel_data: &image_data,
                    width,
                    height,
                }),
                black_box(&mut out_block),
            );
            black_box(&image_data);
            black_box(&out_block);
            black_box(len);
        });
    }
}
