use core::hint::*;
use core::mem;
use core::simd::*;
use core::simd::{cmp::*, num::*};
use std::arch::x86_64::_mm_shuffle_epi8;

// Convert a native-endian number to hybrid-little-endian hex
// The bytes are in little-endian order, but each byte is two hex digits, with the most significant being first.
#[inline(always)]
pub(crate) fn hex4_2le(number: u32) -> [u8; 8] {
    let number_hi = ((number & 0xF0F0F0F0u32) as u64) << (32 - 4);
    let number_lo = ((number & 0x0F0F0F0Fu32) as u64) << 0;
    let number_preconverted = number_hi | number_lo;

    let number_lane = u8x8::from_array(number_preconverted.to_le_bytes());
    let number_full: u8x8 = simd_swizzle!(number_lane, [0, 4, 1, 5, 2, 6, 3, 7]);

    // Somehow, this generates waayyy bigger code
    // const DIGITS: Simd<u8, 16> = u8x16::from_array(*b"0123456789ABCDEF");
    // let ascii = DIGITS.swizzle_dyn(number_full.resize(0xFF));
    // ascii.resize(0xFF).to_array()

    let le_10 = number_full.simd_lt(u8x8::splat(10));
    let to_ascii = le_10.select(u8x8::splat(b'0'), u8x8::splat(b'a' - 10));
    (number_full + to_ascii).to_array()
}

#[inline(always)]
pub(crate) fn hex3_2le(number: u32) -> [u8; 8] {
    let number_hi = ((number & 0x00F0F0F0u32) as u64) << (32 - 4);
    let number_lo = ((number & 0x000F0F0Fu32) as u64) << 0;
    let number_preconverted = number_hi | number_lo;

    let number_lane = u8x8::from_array(number_preconverted.to_le_bytes());
    let number_full: u8x8 = simd_swizzle!(number_lane, [0, 4, 1, 5, 2, 6, 3, 7]);

    // Somehow, this generates waayyy bigger code
    // const DIGITS: Simd<u8, 16> = u8x16::from_array(*b"0123456789ABCDEF");
    // let ascii = DIGITS.swizzle_dyn(number_full.resize(0xFF));
    // ascii.resize(0xFF).to_array()

    #[inline(always)]
    fn array6(x: u8) -> u8x8 {
        u8x8::from_array([x, x, x, x, x, x, 0, 0])
    }
    let le_10 = number_full.simd_lt(u8x8::splat(10));
    let to_ascii = le_10.select(array6(b'0'), array6(b'a' - 10));
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

// TODO: Plan of implementation
// 1. Shift the output to the right by 3
// 2. Add "PX " as SIMD vector to the front
// 3. Implement itoa hex with newline at the end
// 4. Double store unaligned
// 5. Increment
// #[inline(always)]
pub fn itoa_cooard_x2(a: u16, b: u16) -> (u8x16, u32) {
    // let both = u16x16::from_array([0, 0, 0, a, a, a, a, a, 0, 0, 0, b, b, b, b, b]);
    let both = u16x16::from_array([a, a, a, a, a, a, a, a, b, b, b, b, b, b, b, b]);

    const DIV10_16: u16x16 = u16x16::from_array([
        // The leading ones are placeholders and will be zeroed anyway
        1, 1, 1, 10000, 1000, 100, 10, 1, 1, 1, 1, 10000, 1000, 100, 10, 1,
    ]);
    let digits = both / DIV10_16;
    // Placing the and here after the division makes the code more optimal (instead of at both =)
    let digits = digits
        & u16x16::from_array([
            0x0, 0x0, 0x0, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0x0, 0x0, 0x0, 0xFFFF, 0xFFFF,
            0xFFFF, 0xFFFF, 0xFFFF,
        ]);
    let digits_10: u8x16 = (digits % u16x16::splat(10)).cast();
    let digits_10 = digits_10
        + u8x16::from_array([
            0,
            0,
            0,
            // We add the ascii 0 to digits only. There will be a gap in the middle,
            // which we fill with a space. However, we can't conditionally insert
            // at a dynamic index, so instead we add a space everywhere.
            // '0' - space + space -> digit; \0 + space -> space :)
            b'0'.wrapping_sub(b' '),
            b'0'.wrapping_sub(b' '),
            b'0'.wrapping_sub(b' '),
            b'0'.wrapping_sub(b' '),
            b'0'.wrapping_sub(b' '),
            0,
            0,
            0,
            b'0'.wrapping_sub(b' '),
            b'0'.wrapping_sub(b' '),
            b'0'.wrapping_sub(b' '),
            b'0'.wrapping_sub(b' '),
            b'0'.wrapping_sub(b' '),
        ]);

    let where_nz = digits.simd_ne(u16x16::splat(0)).to_bitmask() as u32;
    // Each number must have at least *1* digit (even if x or y are zero),
    // so the least-significant digit cannot be "equal to zero".
    // I.e. each number has length at least length 1 <-> 8 - len(a) <= 7;
    // Note that the "zero" positions of where_nz (the three leading empty digits of each group) are guaranteed
    // to be zero.
    let where_nz = where_nz | 0b1000000010000000;
    let alen_8 /* 8 - len(a) */ = ((where_nz & 0xFF) as u8).trailing_zeros();
    let alen = 8 - alen_8;
    let blen_8 /* 8 - len(b) */ = ((where_nz >> 8) as u8).trailing_zeros();
    let total_len = where_nz.count_ones() /* whitespace */ + 1;

    // a_aligned = "digits_10"[left_half] << (shift to the left visually, i.e. towards least-significant digits) (8 - len(a))
    // This way, a's number goes to the very left.
    // We also, at this point, completely ignore b's decimal repr
    let a_aligned: u8x16 = unsafe {
        _mm_shuffle_epi8(
            digits_10.into(),
            (u8x16::from_array([
                // len(a) <= 5, so we care about only the first 5 digits. The rest we mask out so that we get less noise
                0, 1, 2, 3, 4, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
            ]) + u8x16::splat(alen_8 as u8))
            .into(),
        )
        .into()
    };
    // We do the same for b, but with a few caveats:
    // 1. We of course take the digits from the second half
    // 2. We then shift them to the _right_ (visually!) again by len(a) (sic!), so we don't overwrite eachother.
    // digits << 8 "upper half" << (8 - len(b)) >> len(a)
    let b_aligned: u8x16 = unsafe {
        _mm_shuffle_epi8(
            digits_10.into(),
            // Only 11 elements in the array are technically needed, since two u16s cannot possibly be more than 10 digits
            // (We want to have a space here in the future)
            (u8x16::from_array([8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23])
                + u8x16::splat(
                    (blen_8 as i32 - (alen as i32) - 1/* Leave 1 room for the whitespace */) as i8
                        as u8,
                ))
            .into(),
        )
        .into()
    };

    // We take the first a_len digits from a, and the rest from b.
    let a_digits_mask = u8x16::from_array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15])
        .simd_le(u8x16::splat(alen as u8));
    let comb_digits = a_digits_mask.select(a_aligned, b_aligned);

    // Finally, we have to insert the space.
    let comb_digits = comb_digits + u8x16::splat(b' ');

    (comb_digits, total_len)
}

/// Generates a PX command (ascii) for an RGB pixel.
/// Returns: the resulting length
#[inline(always)]
pub(crate) unsafe fn write_px_rgba(out: *mut u8, x: u16, y: u16, pixel: u32) -> usize {
    // let (digits, a_tz, b_tz) = itoa_cooard_x2(x, y);

    // let shuffle = u8x16::from_array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    0
}

#[cfg(test)]
mod tests {
    use crate::encoding_helpers::{hex4_2le, itoa_cooard_x2, itoa_coord_simd, write_px_rgba};
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
    fn test_atoi_x2() {
        for (a, b) in [(12345, 23456), (300, 300), (10, 200), (310, 30), (0, 1)] {
            let (encoded, len) = itoa_cooard_x2(a, b);
            let encoded_b = &encoded.to_array()[..len as usize];
            assert_eq!(encoded_b, format!("{a} {b}").as_bytes(), "a: {a}, b: {b}")
        }
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
