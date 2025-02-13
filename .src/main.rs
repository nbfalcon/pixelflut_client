use cmd_fill::cmd_fill_with_color;
use pixelflut::{PixelflutClient, RGBA};

pub mod blit_image;
pub mod cmd_fill;
pub mod pixelflut;
pub mod pixelflut_builder;
pub mod pixelflut_gstreamer;

// fn main() {
//     let mut client = PixelflutClient::connect("127.0.0.1:4000").unwrap();
//     let color = RGBA {
//         r: 0xFF,
//         g: 0xFF,
//         b: 0x0,
//         a: 0x0,
//     };
//     let fill = cmd_fill_with_color(color.clone(), 128, 128);
//     let fill_s = String::from_utf8(fill.clone()).unwrap();

//     let rgba = format!("{color}"); // TODO: HAcky?
//     let mut result = Vec::<u8>::new();
//     for y in 0..128 {
//         for x in 0..128 {
//             let r = format!("PX {x} {y} {rgba}\r\n");
//             client.send(r.as_bytes());
//         }
//     }
// }
