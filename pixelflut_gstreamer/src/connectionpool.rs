use bit_set::BitSet;
use regex::Regex;
use std::net::IpAddr;
use std::{
    io::{self, Write},
    mem::MaybeUninit,
    net::*,
    os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd},
    ptr,
    sync::nonpoison::{Mutex, RwLock},
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConnectionVia {
    Any,
    ViaIP(IpAddr),
    ViaDevice(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionTuple {
    host: String,
    multiplicity: usize,
    via: ConnectionVia,
}

impl ConnectionTuple {
    pub fn parse(s: &str) -> Option<Vec<ConnectionTuple>> {
        if s == "" {
            return Some(Vec::new());
        }
        if s.trim().is_empty() {
            return None;
        }

        let re = Regex::new(
            r#"(?x)
    (?P<host>[A-Za-z0-9.\-:\[\]]+)   # hostname or ip address
    (?:                              
        @ (?P<via_ip>  [^\#/,]+ )     # @ip
      | / (?P<via_dev> [^\#/,]+ )     # /netdev
    )?
    (?:\#(?P<mult>\d+))?        # #multiplicity
"#,
        )
        .unwrap();

        let mut results = Vec::new();
        for entry in s.split(',') {
            let entry = entry.trim();
            let Some(caps) = re.captures(entry) else {
                return None;
            };
            let host = caps.name("host").map(|m| m.as_str().to_string())?;
            let multiplicity = caps
                .name("mult")
                .map(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(Some(1))?;

            let via = if let Some(ip) = caps.name("via_ip") {
                if let Ok(addr) = ip.as_str().parse::<IpAddr>() {
                    ConnectionVia::ViaIP(addr)
                } else {
                    ConnectionVia::Any
                }
            } else if let Some(dev) = caps.name("via_dev") {
                ConnectionVia::ViaDevice(dev.as_str().to_string())
            } else {
                ConnectionVia::Any
            };

            results.push(ConnectionTuple {
                host,
                multiplicity,
                via,
            });
        }

        if results.is_empty() {
            None
        } else {
            Some(results)
        }
    }
}

#[derive(Default)]
struct ConnectionPoolInner {
    all_connections: Vec<Mutex<TcpStream>>,
}

#[derive(Default)]
pub struct ConnectionPool {
    inner: RwLock<ConnectionPoolInner>,
}

impl ConnectionPool {
    pub fn set_connections(&self, connections: &[ConnectionTuple]) -> io::Result<()> {
        let all_connections = connect_all(connections)?;
        let mut connections = self.inner.write();
        connections.all_connections.clear();
        connections
            .all_connections
            .extend(all_connections.into_iter().map(Mutex::new));
        Ok(())
    }

    pub fn send(&self, robin_idx: usize, data: &[u8]) -> io::Result<()> {
        let connections = self.inner.read();
        let mut dest =
            connections.all_connections[robin_idx % connections.all_connections.len()].lock();
        dest.write_all(data)?;
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.inner.read().all_connections.is_empty()
    }
}

fn connect_all(connections: &[ConnectionTuple]) -> io::Result<Vec<TcpStream>> {
    let mut all_connections = Vec::new();
    for tuple in connections {
        connect_tuple(&mut all_connections, tuple)?;
    }
    Ok(all_connections)
}

fn connect_tuple(all_connections: &mut Vec<TcpStream>, tuple: &ConnectionTuple) -> io::Result<()> {
    let mut addrs: Vec<SocketAddr> = tuple
        .host
        .to_socket_addrs()
        .map_err(|e| io::Error::new(e.kind(), format!("to_socket_addrs(): {e}")))?
        .collect();

    // We want only the right "kind" of address (ipv4 <-> ipv6 and vice-versa)
    if let &ConnectionVia::ViaIP(addrkind) = &tuple.via {
        assert!(addrkind.is_ipv4() || addrkind.is_ipv6());
        addrs.retain(|e| e.is_ipv4() == addrkind.is_ipv4());
    }

    let addr_is_dead = BitSet::with_capacity(addrs.len());
    let mut addr_next_base = 0;
    for _ in 0..tuple.multiplicity {
        let mut found_stream: Option<TcpStream> = None;
        for addr_i in 0..addrs.len() {
            let addr_i2 = (addr_i + addr_next_base) % addrs.len();
            if addr_is_dead.contains(addr_i2) {
                continue;
            }

            if let Ok(conn) = connect_1(addrs[addr_i2], &tuple.via) {
                addr_next_base += 1;
                found_stream = Some(conn);
                break;
            };
        }

        let Some(found_stream) = found_stream else {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                format!(
                    "Failed to connect to `{}` ({})",
                    tuple.host,
                    io::Error::last_os_error()
                ),
            ));
        };
        all_connections.push(found_stream);
    }

    Ok(())
}

fn connect_1(addr: SocketAddr, via: &ConnectionVia) -> io::Result<TcpStream> {
    match via {
        ConnectionVia::Any => TcpStream::connect(addr),
        ConnectionVia::ViaIP(via_ip) => connect_socket_via_ip(addr, *via_ip),
        ConnectionVia::ViaDevice(via_netdev) => connect_socket_via_netdev(addr, via_netdev),
    }
}

pub fn connect_socket_via_ip(addr: SocketAddr, bind_via: IpAddr) -> io::Result<TcpStream> {
    unsafe {
        let fd = create_socket(addr)?;

        // ---- bind to source ip ----
        let (bind_addr, bind_len) = sockaddr_from_socketaddr(sockaddr_add_port(bind_via, 0));

        if libc::bind(
            fd.as_raw_fd(),
            &bind_addr as *const _ as *const libc::sockaddr,
            bind_len,
        ) < 0
        {
            return Err(io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                format!(
                    "bind(source ip {}) failed: {}",
                    bind_via,
                    io::Error::last_os_error()
                ),
            ));
        }

        // ---- connect ----
        let (dst_addr, dst_len) = sockaddr_from_socketaddr(addr);
        if libc::connect(
            fd.as_raw_fd(),
            &dst_addr as *const _ as *const libc::sockaddr,
            dst_len,
        ) < 0
        {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                format!("connect({}) failed: {}", addr, io::Error::last_os_error()),
            ));
        }

        Ok(TcpStream::from_raw_fd(fd.into_raw_fd()))
    }
}

pub fn connect_socket_via_netdev(addr: SocketAddr, netdev: &str) -> io::Result<TcpStream> {
    unsafe {
        let fd = create_socket(addr)?;

        let dev = std::ffi::CString::new(netdev).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("netdev contains NUL byte: {:?}", netdev),
            )
        })?;

        if libc::setsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_BINDTODEVICE,
            dev.as_ptr() as *const _,
            dev.as_bytes_with_nul().len() as libc::socklen_t,
        ) < 0
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "setsockopt(SO_BINDTODEVICE, {}) failed: {} (need CAP_NET_RAW/root)",
                    netdev,
                    io::Error::last_os_error()
                ),
            ));
        }

        let (dst_addr, dst_len) = sockaddr_from_socketaddr(addr);
        if libc::connect(
            fd.as_raw_fd(),
            &dst_addr as *const _ as *const libc::sockaddr,
            dst_len,
        ) < 0
        {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                format!(
                    "connect({}) via {} failed: {}",
                    addr,
                    netdev,
                    io::Error::last_os_error()
                ),
            ));
        }

        Ok(TcpStream::from_raw_fd(fd.into_raw_fd()))
    }
}

fn create_socket(addr: SocketAddr) -> io::Result<OwnedFd> {
    unsafe {
        let domain = match addr {
            SocketAddr::V4(_) => libc::AF_INET,
            SocketAddr::V6(_) => libc::AF_INET6,
        };
        let fd = libc::socket(domain, libc::SOCK_STREAM, 0);
        if fd < 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("socket() failed: {}", io::Error::last_os_error()),
            ));
        }
        Ok(OwnedFd::from_raw_fd(fd))
    }
}

fn sockaddr_from_socketaddr(addr: SocketAddr) -> (libc::sockaddr_storage, libc::socklen_t) {
    unsafe {
        let mut out: libc::sockaddr_storage = MaybeUninit::zeroed().assume_init();
        let socklen;

        match addr {
            SocketAddr::V4(v4) => {
                let mut storage: libc::sockaddr_in = MaybeUninit::zeroed().assume_init();
                storage.sin_family = libc::AF_INET as _;
                storage.sin_port = v4.port().to_be();
                storage.sin_addr = libc::in_addr {
                    s_addr: u32::from_ne_bytes(v4.ip().octets()).to_be(),
                };
                ptr::write(&mut out as *mut _ as *mut libc::sockaddr_in, storage);
                socklen = size_of_val(&storage);
            }
            SocketAddr::V6(v6) => {
                let mut storage: libc::sockaddr_in6 = MaybeUninit::zeroed().assume_init();
                storage.sin6_family = libc::AF_INET6 as _;
                storage.sin6_port = v6.port().to_be();
                storage.sin6_addr = libc::in6_addr {
                    s6_addr: v6.ip().octets(),
                };
                ptr::write(&mut out as *mut _ as *mut libc::sockaddr_in6, storage);
                socklen = size_of_val(&storage);
            }
        }

        (out, socklen as u32)
    }
}

fn sockaddr_add_port(addr: IpAddr, port: u16) -> SocketAddr {
    match addr {
        IpAddr::V4(ipv4_addr) => SocketAddr::V4(SocketAddrV4::new(ipv4_addr, port)),
        IpAddr::V6(ipv6_addr) => SocketAddr::V6(SocketAddrV6::new(ipv6_addr, port, 0, 0)),
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, ToSocketAddrs};

    use crate::connectionpool::ConnectionTuple;

    #[test]
    pub fn test_sockaddrs() {
        assert_eq!(
            Some(vec![ConnectionTuple {
                host: "127.0.0.1:1234".to_owned(),
                multiplicity: 1,
                via: crate::connectionpool::ConnectionVia::Any
            }]),
            ConnectionTuple::parse("127.0.0.1:1234")
        );

        assert_eq!(
            Some(vec![ConnectionTuple {
                host: "127.0.0.1:1234".to_owned(),
                multiplicity: 20,
                via: crate::connectionpool::ConnectionVia::Any
            }]),
            ConnectionTuple::parse("127.0.0.1:1234#20")
        );
    }
}
