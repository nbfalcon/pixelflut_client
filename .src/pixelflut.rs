use std::{
    fmt,
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    net::{TcpStream, ToSocketAddrs},
};

#[derive(Clone, Copy)]
pub struct RGBA {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

fn hex2(out: &mut fmt::Formatter<'_>, c: u8) -> fmt::Result {
    let lo = c & 0xF;
    let hi = (c & 0xF0) >> 4;
    let chars = [
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F',
    ];

    fmt::Write::write_char(out, chars[hi as usize])?;
    fmt::Write::write_char(out, chars[lo as usize])?;
    Ok(())
}

impl fmt::Display for RGBA {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        hex2(f, self.r)?;
        hex2(f, self.g)?;
        hex2(f, self.b)?;
        Ok(())
    }
}

pub type Coord = u32;

pub struct PixelflutClient {
    socket: TcpStream,

    pub width: Coord,
    pub height: Coord,
}

impl PixelflutClient {
    pub fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<PixelflutClient> {
        let socket = TcpStream::connect(addr)?;

        let mut client = PixelflutClient {
            socket,
            width: 0,
            height: 0,
        };
        client.discover_size()?;

        Ok(client)
    }

    fn discover_size(&mut self) -> io::Result<()> {
        io::Write::write(&mut self.socket, "SIZE\r\n".as_bytes())?;

        let mut response_line = String::new();
        // FIXME: this is horrible (and can end up discarding bytes)
        let mut rxbuf = BufReader::new(&self.socket);
        rxbuf.read_line(&mut response_line)?;

        let mut split = response_line.split_whitespace();
        let w_size = split.next().unwrap(); // FIXME: this is wrong
        let w_width = split.next().unwrap();
        let w_height = split.next().unwrap();

        self.width = w_width.parse().unwrap();
        self.height = w_height.parse().unwrap();

        Ok(())
    }

    pub fn send(&mut self, buf: &[u8]) {
        self.socket.write(buf).unwrap();
    }
}
