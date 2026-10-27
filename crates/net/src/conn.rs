//! Connections: W3GS packets over TCP, and games offered and sought over UDP.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use crate::Packet;

/// A TCP connection's packets: a thread reads them into a channel; writes go straight
/// out.  The connection is gone once `alive` is false.
pub struct Conn {
    stream: TcpStream,
    rx: Receiver<Packet>,
    pub alive: bool,
    pub peer: Option<SocketAddr>,
}

impl Conn {
    pub fn new(stream: TcpStream) -> std::io::Result<Conn> {
        stream.set_nodelay(true)?;
        stream.set_nonblocking(false)?;
        let peer = stream.peer_addr().ok();
        let mut reader = stream.try_clone()?;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                let n = match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                buf.extend_from_slice(&chunk[..n]);
                loop {
                    let len = match Packet::length(&buf) {
                        Ok(Some(len)) if buf.len() >= len => len,
                        Ok(_) => break,
                        Err(e) => {
                            log::warn!("w3gs: {e}");
                            return;
                        }
                    };
                    let bytes: Vec<u8> = buf.drain(..len).collect();
                    match Packet::parse(&bytes) {
                        Ok(p) => {
                            if tx.send(p).is_err() {
                                return;
                            }
                        }
                        Err(e) => log::warn!("w3gs: packet {:#04x}: {e}", bytes[1]),
                    }
                }
            }
        });
        Ok(Conn { stream, rx, alive: true, peer })
    }

    pub fn connect(addr: SocketAddr) -> std::io::Result<Conn> {
        Conn::new(TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(5))?)
    }

    pub fn send(&mut self, p: &Packet) {
        if self.alive && self.stream.write_all(&p.to_bytes()).is_err() {
            self.alive = false;
        }
    }

    /// The packets that have come.
    pub fn poll(&mut self) -> Vec<Packet> {
        let mut out = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(p) => out.push(p),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.alive = false;
                    break;
                }
            }
        }
        out
    }

    /// This end's address, as a joining player tells the host (REQJOIN).
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.stream.local_addr().ok()
    }

    pub fn close(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        self.alive = false;
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

/// Takes TCP connections without waiting.
pub struct Listener(TcpListener);

impl Listener {
    pub fn bind(port: u16) -> std::io::Result<Listener> {
        let l = TcpListener::bind(("0.0.0.0", port))?;
        l.set_nonblocking(true)?;
        Ok(Listener(l))
    }

    pub fn port(&self) -> u16 {
        self.0.local_addr().map_or(0, |a| a.port())
    }

    pub fn accept(&self) -> Option<Conn> {
        match self.0.accept() {
            Ok((s, _)) => Conn::new(s).map_err(|e| log::warn!("w3gs: {e}")).ok(),
            Err(_) => None,
        }
    }
}

/// The UDP side: the game port, where games are sought and offered.  Bound to the game
/// port when it is free; otherwise to any port, which can still seek games and offer one
/// to those it hears from (another program on this computer holds the game port).
pub struct Discovery {
    socket: UdpSocket,
    port: u16,
    /// Bound to the game port itself.
    pub listening: bool,
}

impl Discovery {
    pub fn new(port: u16) -> std::io::Result<Discovery> {
        let (socket, listening) = match UdpSocket::bind(("0.0.0.0", port)) {
            Ok(s) => (s, true),
            Err(_) => (UdpSocket::bind(("0.0.0.0", 0))?, false),
        };
        socket.set_broadcast(true)?;
        socket.set_nonblocking(true)?;
        Ok(Discovery { socket, port, listening })
    }

    /// Sends a packet to everyone on the network's game port (this computer among them);
    /// without a network, to this computer's.  Once only: a game heard from two
    /// addresses would be listed twice.
    pub fn broadcast(&self, p: &Packet) {
        let b = p.to_bytes();
        if self.socket.send_to(&b, ("255.255.255.255", self.port)).is_err() {
            let _ = self.socket.send_to(&b, ("127.0.0.1", self.port));
        }
    }

    pub fn send_to(&self, p: &Packet, to: SocketAddr) {
        let _ = self.socket.send_to(&p.to_bytes(), to);
    }

    /// The packets that have come, with who sent them.
    pub fn poll(&self) -> Vec<(SocketAddr, Packet)> {
        let mut out = Vec::new();
        let mut buf = [0u8; 2048];
        while let Ok((n, from)) = self.socket.recv_from(&mut buf) {
            match Packet::parse(&buf[..n]) {
                Ok(p) => out.push((from, p)),
                Err(e) => log::debug!("w3gs: datagram from {from}: {e}"),
            }
        }
        out
    }
}
