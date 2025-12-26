use crate::base::*;
use crate::encoding_helpers::write_px_rgba;
use core::ptr;

#[repr(C)]
// RGBA_LE32
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub struct PixelflutBuilder<'a> {
    data_slice: &'a mut [u8],
    head_ptr: usize,
}

const PX_MAX_LENGTH: usize = b"PX 65336 65336 RRGGBBAA\r\n".len();

impl<'a> PixelflutBuilder<'a> {
    #[inline(always)]
    pub fn cmd_px(&mut self, x: Coord, y: Coord, color: Color) {
        let len = unsafe {
            write_px_rgba(
                self.slice_head(),
                x,
                y,
                u32::from_le_bytes([color.r, color.g, color.b, color.a]),
            )
        };
        self.add_length(len.into());
    }

    #[inline(always)]
    pub fn cmd_pxb(&mut self, x: Coord, y: Coord, color: Color) {
        debug_assert!(self.check_capacity(1));

        self.add_slice(b"PB");
        self.add_slice(&x.to_le_bytes());
        self.add_slice(&y.to_le_bytes());
        self.add_slice(&[color.r, color.g, color.b, color.a]);
    }

    pub fn with_capacity(data_slice: &'a mut [u8], max_px_count: usize) -> Self {
        // TODO: Make this return Option, we can then return NOT-NEGOTIATED instead of asserting out
        assert!(data_slice.len() >= max_px_count * PX_MAX_LENGTH);
        PixelflutBuilder {
            data_slice,
            head_ptr: 0,
        }
    }

    pub fn with_xy_capacity(data_slice: &'a mut [u8], x: Coord, y: Coord) -> Self {
        PixelflutBuilder::with_capacity(data_slice, (x as usize) * (y as usize))
    }

    pub fn required_size(x: Coord, y: Coord) -> usize {
        (x as usize) * (y as usize) * PX_MAX_LENGTH + PX_MAX_LENGTH
    }

    pub fn check_capacity(&self, n_pixels: usize) -> bool {
        self.data_slice.len() - self.head_ptr >= PX_MAX_LENGTH * n_pixels
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data_slice[..self.head_ptr]
    }

    // FIXME: unsound
    #[inline(always)]
    fn add_slice(&mut self, append_me: &[u8]) {
        unsafe {
            let p = self.data_slice.as_mut_ptr().offset(self.head_ptr as isize);
            ptr::copy_nonoverlapping(append_me.as_ptr(), p, append_me.len());
            self.head_ptr += append_me.len();
        }
    }

    #[inline(always)]
    unsafe fn slice_head(&mut self) -> *mut u8 {
        self.data_slice
            .as_mut_ptr()
            .byte_offset(self.head_ptr as isize)
    }

    fn add_length(&mut self, length: usize) {
        self.head_ptr += length;
    }
}
