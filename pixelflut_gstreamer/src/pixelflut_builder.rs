use std::ptr;

#[repr(C)]
// RGBA_LE32
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub type Coord = u16;

#[inline(always)]
fn hex2(digit: u8) -> [u8; 2] {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let dhi = DIGITS[(digit >> 4) as usize];
    let dlo = DIGITS[(digit & 0xF) as usize];
    [dhi, dlo]
}

fn itoa_coord(mut c: u16) -> arrayvec::ArrayVec<u8, 5> {
    let mut r = arrayvec::ArrayVec::<u8, 5 /* 65336 fits into 5 digits */>::new();
    // Max 5 digits
    for _ in 0..5 {
        let digit = (c % 10) as u8;
        unsafe {
            r.push_unchecked(digit + b'0');
        }
        c /= 10;

        if c == 0 {
            break;
        }
    }
    r.reverse();
    r
}

const PX_MAX_LENGTH: usize = b"PX 65336 65336 RRGGBBAA\r\n".len();

pub struct PixelflutBuilder<'a> {
    data_slice: &'a mut [u8],
    head_ptr: usize,
}

impl<'a> PixelflutBuilder<'a> {
    #[inline(always)]
    fn add_slice(&mut self, append_me: &[u8]) {
        unsafe {
            let p = self.data_slice.as_mut_ptr().offset(self.head_ptr as isize);
            ptr::copy_nonoverlapping(append_me.as_ptr(), p, append_me.len());
            self.head_ptr += append_me.len();
        }
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

    pub fn cmd_px(&mut self, x: Coord, y: Coord, color: Color) {
        assert!(self.check_capacity(1));

        self.add_slice(b"PX ");
        self.add_slice(&itoa_coord(x));
        self.add_slice(b" ");
        self.add_slice(&itoa_coord(y));
        self.add_slice(b" ");
        self.add_slice(&hex2(color.r));
        self.add_slice(&hex2(color.g));
        self.add_slice(&hex2(color.b));
        self.add_slice(&hex2(color.a));
        self.add_slice(b"\r\n");
    }

    pub fn check_capacity(&self, n_pixels: usize) -> bool {
        self.data_slice.len() - self.head_ptr >= PX_MAX_LENGTH * n_pixels
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data_slice[..self.head_ptr]
    }
}
