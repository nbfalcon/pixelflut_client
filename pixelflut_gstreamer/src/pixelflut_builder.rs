use core::simd::{simd_swizzle, u8x4, u8x8};
use std::{ptr, simd::cmp::SimdPartialOrd};

#[repr(C)]
// RGBA_LE32
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub type Coord = u16;

// Convert a native-endian number to hybrid-little-endian hex
// The bytes are in little-endian order, but each byte is two hex digits, with the most significant being first.
#[inline(always)]
fn hex4_2le(number: u32) -> [u8; 8] {
    let number_hi = (number & 0xF0F0F0F0) >> 4;
    let number_lo = number & 0x0F0F0F0F;

    let number_hi_s = u8x4::from(number_hi.to_le_bytes());
    let number_lo_s = u8x4::from(number_lo.to_le_bytes());
    // NOTE: This compiles to one interleave. .interleave() gives us two vectors, and I don't know how to work with that
    let number_full: u8x8 = simd_swizzle!(number_hi_s, number_lo_s, [0, 4, 1, 5, 2, 6, 3, 7]);

    // Somehow, this generates waayyy bigger code
    // const DIGITS: Simd<u8, 16> = u8x16::from_array(*b"0123456789ABCDEF");
    // let ascii = DIGITS.swizzle_dyn(number_full.resize(0xFF));
    // ascii.resize(0xFF).to_array()

    let le_10 = number_full.simd_lt(u8x8::splat(10));
    let to_ascii = le_10.select(u8x8::splat(b'0'), u8x8::splat(b'A' - 10));
    (number_full + to_ascii).to_array()
}

#[inline(always)]
fn itoa_coord(mut c: u16) -> [u8; 5] {
    let mut result = [0u8; 5];
    result[4] = (c % 10) as u8 + b'0';
    c /= 10;
    result[3] = (c % 10) as u8 + b'0';
    c /= 10;
    result[2] = (c % 10) as u8 + b'0';
    c /= 10;
    result[1] = (c % 10) as u8 + b'0';
    c /= 10;
    result[0] = c as u8 + b'0';
    // FIXME: omit leading zeroes
    result
}

pub struct PixelflutBuilder<'a> {
    data_slice: &'a mut [u8],
    head_ptr: usize,
}

const PX_MAX_LENGTH: usize = b"PX 65336 65336 RRGGBBAA\r\n".len();
impl<'a> PixelflutBuilder<'a> {
    pub fn cmd_px(&mut self, x: Coord, y: Coord, color: Color) {
        // FIXME: unsound
        debug_assert!(self.check_capacity(1));

        self.add_slice(b"PX ");
        self.add_slice(&itoa_coord(x));
        self.add_slice(b" ");
        self.add_slice(&itoa_coord(y));
        self.add_slice(b" ");
        self.add_slice(&hex4_2le(u32::from_le_bytes([
            color.r, color.g, color.b, color.a,
        ])));
        self.add_slice(b"\r\n");
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
        (x as usize) * (y as usize) * PX_MAX_LENGTH
    }

    pub fn check_capacity(&self, n_pixels: usize) -> bool {
        self.data_slice.len() - self.head_ptr >= PX_MAX_LENGTH * n_pixels
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data_slice[..self.head_ptr]
    }

    #[inline(always)]
    fn add_slice(&mut self, append_me: &[u8]) {
        unsafe {
            let p = self.data_slice.as_mut_ptr().offset(self.head_ptr as isize);
            ptr::copy_nonoverlapping(append_me.as_ptr(), p, append_me.len());
            self.head_ptr += append_me.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::pixelflut_builder::{hex4_2le, itoa_coord};

    #[test]
    fn test_encoding_helpers() {
        assert_eq!(hex4_2le(0xAABBCCDD), *b"DDCCBBAA");
        assert_eq!(itoa_coord(10050), *b"10050");
    }
}
