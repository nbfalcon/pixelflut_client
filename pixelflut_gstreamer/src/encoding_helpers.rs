use core::mem;
use core::simd::{simd_swizzle, u8x4, u8x8};
use std::ptr;
use std::simd::{
    cmp::{SimdPartialEq, SimdPartialOrd},
    num::SimdUint,
    u16x8,
};

// Convert a native-endian number to hybrid-little-endian hex
// The bytes are in little-endian order, but each byte is two hex digits, with the most significant being first.
#[inline(always)]
pub(crate) fn hex4_2le(number: u32) -> [u8; 8] {
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
    let to_ascii = le_10.select(u8x8::splat(b'0'), u8x8::splat(b'a' - 10));
    (number_full + to_ascii).to_array()
}

#[inline(always)]
pub(crate) fn itoa_coord(mut c: u16) -> [u8; 5] {
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

/// Converts a 16-bit number to a decimal representation
/// Returns: an array representing the ascii bytes of the number + a length.
/// The array is left-aligned, i.e. array[0] is the most-significant-digit
#[inline(always)]
pub(crate) fn itoa_coord_simd(n: u16) -> ([u8; 8], u8) {
    const DIV10_8: u16x8 = u16x8::from_array([1, 1, 1, 10000, 1000, 100, 10, 1]);
    let cx8 = u16x8::splat(n);
    let as_digits_pre = cx8 / DIV10_8;
    let as_digits: u8x8 = (as_digits_pre % u16x8::splat(10)).cast();
    let as_digits_ascii = as_digits + u8x8::splat(b'0');

    let leading_zeroes = (as_digits.simd_ne(u8x8::splat(0)).to_bitmask() as u8) & 0b11111000;
    let leading_zeroes = leading_zeroes | (1 << 7); // The most significant bit must always be included, since "0" must be written out as well
    let n_lz = leading_zeroes.trailing_zeros() as u8;
    let as_digits_ascii_n: u64 = unsafe { mem::transmute(as_digits_ascii) };
    // Now we remove the leading zeroes
    let no_lz = as_digits_ascii_n >> (n_lz * 8);

    let length = 8 - n_lz;
    let out: [u8; 8] = no_lz.to_le_bytes();
    (out, length)
}

/// Generates a PX command (ascii) for an RGB pixel.
/// Returns: the resulting length
#[inline(always)]
pub(crate) unsafe fn write_px_rgba(out: *mut u8, x: u16, y: u16, pixel: u32) -> u8 {
    let (x, x_len) = itoa_coord_simd(x);
    let (y, y_len) = itoa_coord_simd(y);
    let x = u64::from_le_bytes(x);
    let y = u64::from_le_bytes(y);
    let hex = u64::from_le_bytes(hex4_2le(pixel | 0xFF000000u32));

    // Add formattig
    let x_px = (x << 24) | (u32::from_le_bytes(*b"PX \0") as u64);
    let y_spc = (y << 8) | (b' ' as u64) | (b' ' as u64) << ((y_len + 1) * 8);

    unsafe {
        ptr::write_unaligned(out.byte_offset(0) as *mut _, x_px);
        ptr::write_unaligned(out.byte_offset((x_len + 3) as isize) as *mut _, y_spc);
        ptr::write_unaligned(
            out.byte_offset((x_len + 3 + y_len + 2) as isize) as *mut _,
            hex,
        );
        ptr::write_unaligned(
            out.byte_offset((x_len + 3 + y_len + 2 + 8) as isize) as *mut _,
            *b"\n",
        );
    }
    let total = 3 + x_len + 1 + y_len + 1 + 8 + 1 as u8;
    total
}

#[cfg(test)]
mod tests {
    use crate::encoding_helpers::{hex4_2le, itoa_coord_simd, write_px_rgba};
    use test::Bencher;

    #[test]
    fn test_encoding_helpers() {
        assert_eq!(hex4_2le(0xAABBCCDD), *b"ddccbbaa");

        let (out, len) = itoa_coord_simd(65300);
        assert_eq!(out[..len as usize], *b"65300");
        assert_eq!(len, 5);

        let (out, len) = itoa_coord_simd(1);
        assert_eq!(out[..len as usize], *b"1");
        assert_eq!(len, 1);
    }

    #[test]
    fn test_encode_px_command() {
        let mut out = [0u8; 128];
        // Note: we have little-endian
        let l = unsafe { write_px_rgba((&mut out).as_mut_ptr(), 230, 5000, 0xbbaaff) };
        assert_eq!(
            String::from_utf8_lossy(&out[..l as usize]),
            "PX 230 5000 ffaabbff\r\n"
        );

        let l = unsafe { write_px_rgba((&mut out).as_mut_ptr(), 0, 1, 0xbbaaff) };
        assert_eq!(
            String::from_utf8_lossy(&out[..l as usize]),
            "PX 0 1 ffaabbff\r\n"
        );

        for x in 0..1920 {
            for y in 0..1080 {
                // println!("x = {x}, y = {y}");
                unsafe { write_px_rgba((&mut out).as_mut_ptr(), x, y, 0xbbaaff) };
            }
        }
    }

    #[bench]
    fn benchmark_itoa_simple(b: &mut Bencher) {
        b.iter(|| {
            for _ in 0..(1920 * 1080) {
                let mut out = [0u8; 32];
                let len = unsafe {
                    write_px_rgba(
                        (&mut out).as_mut_ptr(),
                        std::hint::black_box(6400),
                        std::hint::black_box(200),
                        std::hint::black_box(0xFFAABB),
                    )
                };
                std::hint::black_box(out);
                std::hint::black_box(len);
            }
        });
    }
}
