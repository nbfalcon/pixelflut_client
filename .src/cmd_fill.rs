use std::io::Write;

use crate::pixelflut::{Coord, RGBA};

pub fn cmd_fill_with_color(color: RGBA, width: Coord, height: Coord) -> Vec<u8> {
    let rgba = format!("{color}"); // TODO: HAcky?
    let mut result = Vec::<u8>::new();
    for y in 0..height {
        for x in 0..width {
            write!(result, "PX {x} {y} {rgba}\r\n").unwrap();
        }
    }
    result
}