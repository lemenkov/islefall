// SPDX-License-Identifier: Apache-2.0
//! What clients and the server say to each other, and how it is framed.
//!
//! Islefall plays in lockstep: clients send commands, the server stamps
//! them into numbered turns and sends every turn's list to everyone, and
//! each client advances its own simulation by the turn's ticks. Frames are
//! a little-endian `u32` length followed by the postcard bytes of a
//! message; the same framing works on a blocking stream (the client) and
//! an async one (the server, with the `tokio` feature).

use std::io::{self, Read, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use islefall_sim::Command;

/// Bumped whenever a message changes shape.
pub const PROTOCOL: u32 = 1;
/// No frame may be longer than this; a snapshot of a large world is far under it.
pub const MAX_FRAME: usize = 16 << 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub id: u8,
    pub name: String,
    pub ready: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// First message: who this is and what data it runs, so the server can
    /// refuse a client whose rules, scripts or map differ from the others'.
    Hello { protocol: u32, name: String, map: String, data_hash: u64 },
    Ready(bool),
    /// A command for the next turn.
    Command(Command),
    /// The client's world hash after applying `turn`, for desync checks.
    Hash { turn: u64, hash: u64 },
    Ping(u64),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome { player: u8, turn_ticks: u32, hash_every_turns: u32 },
    Reject(String),
    Lobby { players: Vec<PlayerInfo> },
    Start { map: String },
    /// Every command given for this turn, in the server's order.
    Turn { turn: u64, commands: Vec<(u8, Command)> },
    /// Clients disagreed about `turn`: each player's hash.
    Desync { turn: u64, hashes: Vec<(u8, u64)> },
    Left(u8),
    Pong(u64),
}

pub fn encode<T: Serialize>(msg: &T) -> io::Result<Vec<u8>> {
    let body = postcard::to_stdvec(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if body.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too long"));
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode<T: DeserializeOwned>(body: &[u8]) -> io::Result<T> {
    postcard::from_bytes(body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn frame_len(header: [u8; 4]) -> io::Result<usize> {
    let len = u32::from_le_bytes(header) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too long"));
    }
    Ok(len)
}

/// Write one message to a blocking stream.
pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()> {
    w.write_all(&encode(msg)?)?;
    w.flush()
}

/// Read one message from a blocking stream.
pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> io::Result<T> {
    let mut header = [0u8; 4];
    r.read_exact(&mut header)?;
    let mut body = vec![0u8; frame_len(header)?];
    r.read_exact(&mut body)?;
    decode(&body)
}

#[cfg(feature = "tokio")]
pub async fn send<W: tokio::io::AsyncWrite + Unpin, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()> {
    use tokio::io::AsyncWriteExt;
    w.write_all(&encode(msg)?).await?;
    w.flush().await
}

#[cfg(feature = "tokio")]
pub async fn recv<R: tokio::io::AsyncRead + Unpin, T: DeserializeOwned>(r: &mut R) -> io::Result<T> {
    use tokio::io::AsyncReadExt;
    let mut header = [0u8; 4];
    r.read_exact(&mut header).await?;
    let mut body = vec![0u8; frame_len(header)?];
    r.read_exact(&mut body).await?;
    decode(&body)
}

/// FNV-1a over bytes, for the data hash clients present.
pub fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use islefall_sim::Cell;

    #[test]
    fn frames_round_trip() {
        let msg = ServerMsg::Turn { turn: 7, commands: vec![(1, Command::Move { unit: 2, to: Cell::new(3, 4) })] };
        let bytes = encode(&msg).unwrap();
        let mut cursor = io::Cursor::new(bytes);
        let back: ServerMsg = read_frame(&mut cursor).unwrap();
        assert_eq!(back, msg);
        let mut out = Vec::new();
        write_frame(&mut out, &ClientMsg::Ping(9)).unwrap();
        assert_eq!(&out[..4], &(out.len() as u32 - 4).to_le_bytes());
        assert_ne!(fnv64(b"a"), fnv64(b"b"));
    }
}
