use core::mem;
use core::simd::{simd_swizzle, u8x4, u8x8};
use std::arch::x86_64::_mm_storeu_si128;
use std::mem::MaybeUninit;
use std::simd::{mask8x16, u64x2};
use std::{
    arch::x86_64,
    ptr,
    simd::{
        cmp::{SimdPartialEq, SimdPartialOrd},
        num::SimdUint,
        u16x16, u16x8, u8x16,
    },
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

#[inline(always)]
pub(crate) fn itoa_coord_simd(c: u16) -> [u8; 5] {
    const DIV10_8: u16x8 = u16x8::from_array([1, 1, 1, 10000, 1000, 100, 10, 1]);
    let cx8 = u16x8::splat(c);
    let as_digits_pre = cx8 / DIV10_8;
    let as_digits: u8x8 = (as_digits_pre % u16x8::splat(10)).cast();
    let as_digits_ascii = as_digits + u8x8::splat(b'0');

    let mut result = [0u8; 5];
    result.copy_from_slice(&as_digits_ascii[3..8]);
    result
}

#[inline(always)]
pub(crate) fn itoa_coord_simd_l(n: u16, out: &mut [u8; 8]) -> u8 {
    const DIV10_8: u16x8 = u16x8::from_array([1, 1, 1, 10000, 1000, 100, 10, 1]);
    let cx8 = u16x8::splat(n);
    let as_digits_pre = cx8 / DIV10_8;
    let as_digits: u8x8 = (as_digits_pre % u16x8::splat(10)).cast();
    let as_digits_ascii = as_digits + u8x8::splat(b'0');

    let leading_zeroes = as_digits.simd_ne(u8x8::splat(0)).to_bitmask() as u8;
    let leading_zeroes = leading_zeroes | (1 << 7); // The most significant bit must always be included
    let n_lz = leading_zeroes.trailing_zeros() as u8;
    let as_digits_ascii_n: u64 = unsafe { mem::transmute(as_digits_ascii) };
    // Now we remove the leading zeroes
    let no_lz = as_digits_ascii_n >> (n_lz * 8);

    out.copy_from_slice(&no_lz.to_le_bytes());
    8 - n_lz
}

#[inline(always)]
pub(crate) fn itoa_coord_simd2_avx512(a: u16, b: u16, out: *mut u8) -> u8 {
    // FIXME: unfinished
    const DIV10: u16x16 = u16x16::from_array([
        1, 1, 1, 10000, 1000, 100, 10, 1, 1, 1, 1, 10000, 1000, 100, 10, 1,
    ]);

    // Both numbers concat'd
    let cx8 = simd_swizzle!(
        u16x8::splat(a),
        u16x8::splat(b),
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
    );
    let as_digits_pre = cx8 / DIV10;
    // Both numbers concat'd (ignoring the leaidng ones in DIV10) converted to digits (non-ascii)
    let as_digits: u8x16 = (as_digits_pre % u16x16::splat(10)).cast();
    // This is the actual digits we have. We mask of the [1,1,1] parts because they cannot count.
    let nz_mask =
        (as_digits_pre.simd_ne(u16x16::splat(0))).to_bitmask() as u16 & 0b0001111100011111u16;
    // FIXME: we assume we are non-zero (mask would have to be ored so tha there is a 1 in the lsb of a and b always, which would not be for zero)
    let as_digits_ascii = as_digits + u8x16::splat(b'0');

    // FIXME: we can make this even faster by encoding the "PX" prefix in this function as well.
    unsafe {
        let compressed = x86_64::_mm_maskz_compress_epi8(nz_mask, mem::transmute(as_digits_ascii));
        _mm_storeu_si128(out as *mut _, compressed);
    }

    let length = nz_mask.count_ones() as u8;
    length
}

#[inline(always)]
// FIXME: make it safe by specing out correctly
pub(crate) fn itoa_coord_simd2_sse(a: u16, b: u16, out: *mut u8) -> u8 {
    const DIV10: u16x16 = u16x16::from_array([
        1, 1, 1, 10000, 1000, 100, 10, 1, 1, 1, 1, 10000, 1000, 100, 10, 1,
    ]);
    const DIV10_MASK: u16 = 0b0001111100011111u16;
    // Both numbers concat'd
    let cx8: u16x16 = simd_swizzle!(
        u16x8::splat(a),
        u16x8::splat(b),
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
    );
    let as_digits_pre: u16x16 = cx8 / DIV10;
    let as_digits: u8x16 = (as_digits_pre % u16x16::splat(10)).cast();

    // This is the actual digits we have. We mask of the [1,1,1] parts because they cannot count.
    let mut nz_mask = (as_digits_pre.simd_ne(u16x16::splat(0))).to_bitmask() as u16 & DIV10_MASK;
    nz_mask |= (1 << 0) | (1 << 8);
    let mask_a = (nz_mask >> 8) as u8;
    let mask_b = (nz_mask & 0xFF) as u8;
    let a_lz = mask_a.leading_zeros() as u8;
    let b_lz = mask_b.leading_zeros() as u8;

    let strip_junk_mask = ((0xFFu8 << a_lz) as u16) << 0 | ((0xFFu8 << b_lz) as u16) << 8;
    let as_digits_ascii = as_digits + u8x16::splat(b'0');
    let as_digits_clean =
        mask8x16::from_bitmask(strip_junk_mask.into()).select(as_digits_ascii, u8x16::splat(0));

    unsafe {
        let ascii_2: u64x2 = mem::transmute(as_digits_clean);
        let [num_a, num_b] = ascii_2.to_array();

        // whitespace: insert it in num_a
        let num_a = num_a >> 8 | (b' ' as u64) << 56;
        let a_lz = a_lz - 1;

        // let a_ledge = num_a >> (a_lz * 8);
        // let b_ledge = num_b >> (b_lz * 8);
        // let write_2 = b_ledge >> (a_lz * 8);
        // let b_fora = b_ledge >> ((8 - a_lz) * 8);
        // let write_1 = a_ledge | b_fora;
        // ptr::write_unaligned(out as *mut _, [write_1, write_2]);

        let combined = (num_a as u128) | ((num_b >> (b_lz * 8)) as u128) << 64;
        let combined = combined >> (a_lz * 8);
        ptr::write_unaligned(out as *mut _, combined);
    }

    let length = strip_junk_mask.count_ones() as u8;
    let length = length + 1; // whitespace: +1
    length
}

#[cfg(test)]
mod tests {
    use crate::conv_utils::{
        hex4_2le, itoa_coord, itoa_coord_simd, itoa_coord_simd2_sse, itoa_coord_simd_l,
    };
    use test::Bencher;

    #[test]
    fn test_encoding_helpers() {
        assert_eq!(hex4_2le(0xAABBCCDD), *b"ddccbbaa");
        assert_eq!(itoa_coord(10050), *b"10050");
        assert_eq!(itoa_coord_simd(10050), *b"10050");

        let mut out = [0u8; 8];
        let len = itoa_coord_simd_l(65300, &mut out);
        assert_eq!(out[..len as usize], *b"65300");
        assert_eq!(len, 5);
    }

    #[test]
    fn simd_encoding_helpers() {
        let mut out = [0u8; 32];
        let n1 = 60322;
        let n2 = 65321;
        let len = itoa_coord_simd2_sse(n1, n2, &mut out as *mut u8);
        let out2 = String::from_utf8_lossy(&out[..len.into()]);
        assert_eq!(format!("{n1} {n2}"), out2);
    }

    #[bench]
    fn benchmark_itoa_simple(b: &mut Bencher) {
        b.iter(|| {
            let mut out = [0u8; 32];
            for _ in 0..(1920 * 1080) {
                let mut i = 0;
                unsafe {
                    i += itoa_coord_simd_l(
                        test::black_box(65300),
                        (&mut out[i as usize..i as usize + 8]).try_into().unwrap(),
                    );
                    *out.get_unchecked_mut(i as usize) = b' ';
                    i += itoa_coord_simd_l(
                        test::black_box(65321),
                        (&mut out[i as usize..i as usize + 8])
                            .try_into()
                            .unwrap_unchecked(),
                    );
                    let out_hex: &mut [u8; 8] = out.get_unchecked_mut(i as usize..i as usize + 8).try_into().unwrap_unchecked();
                    i += 8;
                    *out_hex = hex4_2le(0xFFFFAABB);

                    let out_nl: &mut [u8; 2] = &mut out[i as usize..i as usize + 2].try_into().unwrap_unchecked();
                    *out_nl = *b"\r\n";
                    i += 2;
                }
                test::black_box(i);
                test::black_box(out);
            }
        });
    }

    #[bench]
    fn benchmark_itoa_simd(b: &mut Bencher) {
        b.iter(|| {
            let mut out = [0u8; 32];
            for _ in 0..(1920 * 1080) {
                let len = itoa_coord_simd2_sse(
                    test::black_box(6500),
                    test::black_box(6521),
                    &mut out as *mut _,
                );
                test::black_box(out);
                test::black_box(len);
            }
        });
    }
}
