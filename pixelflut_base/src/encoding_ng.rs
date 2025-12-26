use crate::base::*;
use crate::encoding_helpers::*;
use core::arch::x86_64::*;
use core::mem::transmute;
use core::ptr;
use core::simd::cmp::*;
use core::simd::*;

#[inline(always)]
fn simd_shift_left(v: u8x16, n: i8) -> u8x16 {
    let idx_id = i8x16::from_array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    let idx = idx_id + i8x16::splat(n); // Lower lanes will choose higher lanes, i.e. higher lanes migrate down

    unsafe {
        // This comparison only works for non-negative idx entries. However, the negative ones were already discarded
        // by _mm_shuffle_epi8 (due to 0x80 bit), so we just need to take care of the larger than 16 entries
        let legal_mask: u8x16 = transmute(_mm_cmplt_epi8(transmute(idx), _mm_set1_epi8(16)));
        let shuffle: u8x16 = transmute(_mm_shuffle_epi8(transmute(v), transmute(idx)));
        legal_mask & shuffle
    }
}

#[inline(always)]
fn simd_shift_right(v: u8x16, n: u8) -> u8x16 {
    debug_assert!(n <= i8::MAX as u8);

    let idx_id = i8x16::from_array([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    // We can optimize by exploiting that we *subtract* from the idx array. This means invalid indexes will
    // actually go negative, which will alreday be discarded by _mm_shuffle_epi8 anyway.
    let idx = idx_id - i8x16::splat(n as i8); // Lower lanes will choose higher lanes, i.e. higher lanes migrate down
    unsafe { transmute(_mm_shuffle_epi8(transmute(v), transmute(idx))) }
}

const fn simd_mask(mask: u16) -> u8x16 {
    let mut mask_a = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        if mask & (1 << i) != 0 {
            mask_a[i] = 0xFF;
        }
        i += 1;
    }
    u8x16::from_array(mask_a)
}

pub unsafe fn encode_offset_command(x: Coord, y: Coord, out: *mut u8) -> SkipLength {
    let xxxxyyyy: u16x8 = transmute(_mm_unpacklo_epi64(
        _mm_set1_epi16(x as i16),
        _mm_set1_epi16(y as i16),
    ));

    let digits = xxxxyyyy / u16x8::from_array([1000, 100, 10, 1, 1000, 100, 10, 1]);
    let digits = digits % u16x8::splat(10);
    // "Almost ascii" - we convert it to ascii, but subtract b' ', so tha the digits
    // can later be added to an "OFFSET" pattern with spaces (the -b' ' cancels out and we are left with ascii digits)
    // let digits_conv: u8x16 = transmute(_mm_cvtepi16_epi8(transmute(digits))); // AVX512
    let digits_conv: u8x16 =
        transmute::<_, u8x16>(_mm_packus_epi16(transmute(digits), _mm_setzero_si128()))
            + u8x16::splat(b'0' - b' ');

    let xxxx: u8x16 = digits_conv & simd_mask(0b00001111);
    let yyyy: u8x16 = digits_conv & simd_mask(0b11110000);

    let leading_zeroes_mask = digits.simd_ne(u16x8::splat(0)).to_bitmask() | 0b10001000;
    let x_lz = (leading_zeroes_mask/* & 0xF */).trailing_zeros(); // We don't have to actually and with 0xF, because of the 1's we or'd in.
    let y_lz = (leading_zeroes_mask >> 4).trailing_zeros();
    debug_assert!(
        x_lz <= 3 /* we can't have 4 leading zeroes, because then we wouldn't write a number! */
    );
    debug_assert!(y_lz <= 3);
    let x_len = 4 - x_lz;
    let y_len = 4 - y_lz;

    // OFFSET x y
    let xxxx_align = simd_shift_right(simd_shift_left(xxxx, x_lz as i8), 7);
    let yyyy_align = simd_shift_right(
        simd_shift_left(yyyy, y_lz as i8 + 4),
        7 /* 'OFFSET ' */ + x_len as u8 + 1, /* space */
    );
    // let xxxx_align = simd_shift_left(xxxx, x_lz as i8 - 7 /* start of x */);
    // let yyyy_align = simd_shift_left(yyyy, y_lz as i8 + 4 - 7 - x_len as i8 - 1 /* space */);
    let offset_command = u8x16::from_array(*b"OFFSET          ") + xxxx_align + yyyy_align;

    let total_len = b"OFFSET  \n".len() + x_len as usize + y_len as usize;
    unsafe {
        ptr::write_unaligned(out as *mut _, offset_command);
        ptr::write_unaligned(out.add(total_len - 1), b'\n');
    }
    total_len
}

pub type MiniCoord = u8; // 0..9
pub type RgbaValue = u32;
pub type RgbValue = u32;
pub type GrayValue = u8;
/// An "output" slice type, large enough so that one can easily implement SIMD shenanigans
/// It might be oversized for the various PX commands, but this is important for SIMD algorithms.
pub type OutSlice<'a> = &'a mut [u8; 16];

pub struct StaticSkip<const N: SkipLength>;
impl<const N: usize> Into<SkipLength> for StaticSkip<N> {
    fn into(self) -> SkipLength {
        N
    }
}

pub fn encode_px_command_lite_rgba(
    x: MiniCoord,
    y: MiniCoord,
    value: RgbaValue,
    out: OutSlice,
) -> StaticSkip<16> {
    let mut px_command: u8x16 = u8x16::from_array(*b"PX 0 0 \0\0\0\0\0\0\0\0\n");
    let hex = u8x8::from_array(hex4_2le(value)).resize::<16>(0);
    px_command += simd_shift_right(hex, 7);

    out.copy_from_slice(px_command.as_array());
    out[3] = x + b'0';
    out[5] = y + b'0';
    StaticSkip
}

pub fn encode_px_command_lite_rgb(
    x: MiniCoord,
    y: MiniCoord,
    value: RgbValue,
    out: OutSlice,
) -> StaticSkip<14> {
    let mut px_command: u8x16 = u8x16::from_array(*b"PX 0 0 \0\0\0\0\0\0\n\0\0");
    let hex = u8x8::from_array(hex3_2le(value)).resize::<16>(0);
    px_command += simd_shift_right(hex, 7);

    out.copy_from_slice(px_command.as_array());
    out[3] = x + b'0';
    out[5] = y + b'0';
    StaticSkip
}

pub fn encode_px_command_lite_gray(
    x: MiniCoord,
    y: MiniCoord,
    value: GrayValue,
    out: OutSlice,
) -> StaticSkip<10> {
    let as_hex: &[u8; 16] = b"0123456789abcdef";
    let g_lo = as_hex[(value & 0xF) as usize];
    let g_hi = as_hex[(value >> 4) as usize];

    out[..8].copy_from_slice(b"PX 0 0 g");
    out[3] += x;
    out[5] += y;
    out[7] = g_hi;
    out[8] = g_lo;
    out[9] = b'\n';
    StaticSkip
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::hint::black_box;
    use rayon::prelude::*;
    use std::io::Write;
    use test::Bencher;

    type SmallVec = arrayvec::ArrayVec<u8, 128>;

    fn helper_encode_offset_command(x: Coord, y: Coord) -> SmallVec {
        let mut r = SmallVec::new();
        unsafe {
            let len = encode_offset_command(x, y, r.as_mut_ptr());
            r.set_len(len);
        }
        r
    }

    fn reference_encode_offset_command(x: Coord, y: Coord) -> SmallVec {
        let mut r = SmallVec::new();
        write!(r, "OFFSET {x} {y}\n").unwrap();
        r
    }

    fn helper_encode_px_command_lite_rgba(
        x: MiniCoord,
        y: MiniCoord,
        value: RgbaValue,
    ) -> SmallVec {
        let mut r = SmallVec::new();
        r.extend(0..16);
        let len =
            encode_px_command_lite_rgba(x, y, value, r.as_mut_slice().try_into().unwrap()).into();
        unsafe {
            r.set_len(len);
        }
        r
    }

    fn helper_encode_px_command_lite_gray(
        x: MiniCoord,
        y: MiniCoord,
        value: GrayValue,
    ) -> SmallVec {
        let mut r = SmallVec::new();
        r.extend(0..16);
        let len =
            encode_px_command_lite_gray(x, y, value, r.as_mut_slice().try_into().unwrap()).into();
        unsafe {
            r.set_len(len);
        }
        r
    }

    #[test]
    pub fn encode_tests_basic() {
        assert_eq!(
            helper_encode_offset_command(123, 4321).as_slice(),
            b"OFFSET 123 4321\n"
        );
        assert_eq!(
            helper_encode_offset_command(1234, 4321).as_slice(),
            b"OFFSET 1234 4321\n"
        );
        assert_eq!(
            helper_encode_offset_command(1, 9999).as_slice(),
            b"OFFSET 1 9999\n"
        );
        assert_eq!(
            helper_encode_px_command_lite_rgba(1, 2, 0xcc23aa).as_slice(),
            b"PX 1 2 aa32cc00\n"
        );
        assert_eq!(
            helper_encode_px_command_lite_gray(1, 2, 0xaf).as_slice(),
            b"PX 1 2 fa\n"
        );
    }

    #[test]
    pub fn encode_offset_exhaustive() {
        (0..=9999).into_par_iter().for_each(|x| {
            for y in 0..=9999 {
                let reference = reference_encode_offset_command(x, y);
                let actual = helper_encode_offset_command(x, y);
                assert_eq!(reference, actual);
            }
        });
    }

    #[bench]
    fn benchmark_encode_offset_simple(b: &mut Bencher) {
        b.iter(|| {
            // Current benchmarks on my laptop indicate: 8ms/1s is spent in OFFSET, which is very good.
            for _ in 0..(1920 * 1080 * 60 / 100) {
                let mut out = black_box([0u8; 32]);
                unsafe {
                    let len = encode_offset_command(
                        black_box(1234),
                        black_box(4321),
                        black_box(out.as_mut_ptr()),
                    );
                    black_box(len);
                }
            }
        });
    }

    #[bench]
    fn benchmark_encode_video_framerate(b: &mut Bencher) {
        b.iter(|| {
            let width: Coord = black_box(1920);
            let height: Coord = black_box(1080);
            let x_base: Coord = black_box(0);
            let y_base: Coord = black_box(0);

            let mut out = [0u8; 32];
            let mut out2 = [0u8; 32];
            for y_chunk in 0..height.div_ceil(10) {
                for x_chunk in 0..width.div_ceil(10) {
                    unsafe {
                        let len = encode_offset_command(
                            x_chunk + x_base,
                            y_chunk + y_base,
                            out.as_mut_ptr(),
                        );
                        black_box(len);
                        black_box(out);
                    }

                    for dy in 0..9 {
                        for dx in 0..9 {
                            let color = black_box(0x00ccbbaa) /* fetch color */;
                            encode_px_command_lite_rgba(
                                dx,
                                dy,
                                color,
                                (&mut out2[..16]).try_into().unwrap(),
                            );
                            black_box(out2);
                        }
                    }
                }
            }
        });
    }
}
