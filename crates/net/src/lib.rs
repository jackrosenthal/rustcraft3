//! Warcraft III's local area network protocol (W3GS), as 1.27b speaks it: games offered
//! and found over UDP, joined and played over TCP.  The packets (`packet`), the game
//! description they carry (`stat`), the setup's slots (`slot`), and the map checksums
//! (`map`) need nothing of the game, so any program can host or join a game with them;
//! `host` and `client` speak the protocol's sequences, leaving the game's rules and
//! its actions' meaning to the program.

pub mod client;
pub mod conn;
pub mod host;
pub mod map;
pub mod packet;
pub mod slot;
pub mod stat;
pub mod wire;

pub use packet::{Chat, GameInfo, Packet, SockAddr};
pub use slot::{Slot, SlotTable};
pub use stat::GameStat;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("packet too short")]
    Short,
    #[error("bad packet: {0}")]
    Bad(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}
