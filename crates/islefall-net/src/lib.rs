// SPDX-License-Identifier: Apache-2.0
//! What clients and the server say to each other, and how it is framed.
//!
//! Islefall plays in lockstep: clients send commands, the server stamps
//! them into numbered turns and sends every turn's list to everyone, and
//! each client advances its own simulation by the turn's ticks. Frames are
//! a little-endian `u32` length followed by the postcard bytes of a
//! message, as tokio-util's length-delimited codec lays them out; client
//! and server both read and write them through [`reader`] and [`writer`].

use std::io;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

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

/// Postcard bytes of a message, checked against [`MAX_FRAME`].
pub fn encode<T: Serialize>(msg: &T) -> io::Result<Bytes> {
    let body = postcard::to_stdvec(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if body.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too long"));
    }
    Ok(Bytes::from(body))
}

pub fn decode<T: DeserializeOwned>(body: &[u8]) -> io::Result<T> {
    postcard::from_bytes(body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// The wire framing: a little-endian `u32` length, then the message.
pub fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder().little_endian().length_field_type::<u32>().max_frame_length(MAX_FRAME).new_codec()
}

pub type Reader<R> = FramedRead<R, LengthDelimitedCodec>;
pub type Writer<W> = FramedWrite<W, LengthDelimitedCodec>;

pub fn reader<R: AsyncRead>(r: R) -> Reader<R> {
    FramedRead::new(r, codec())
}

pub fn writer<W: AsyncWrite>(w: W) -> Writer<W> {
    FramedWrite::new(w, codec())
}

/// Write one message and flush it.
pub async fn send<W: AsyncWrite + Unpin, T: Serialize>(w: &mut Writer<W>, msg: &T) -> io::Result<()> {
    w.send(encode(msg)?).await
}

/// Read one message; end of stream is an error, since every message is
/// expected to arrive whole.
pub async fn recv<R: AsyncRead + Unpin, T: DeserializeOwned>(r: &mut Reader<R>) -> io::Result<T> {
    match r.next().await {
        Some(Ok(body)) => decode(&body),
        Some(Err(e)) => Err(e),
        None => Err(io::Error::new(io::ErrorKind::UnexpectedEof, "connection closed")),
    }
}

/// The data hash clients present: XXH3 over the bytes of the rules, the
/// scripts and the map as loaded.
pub fn data_hash(bytes: &[u8]) -> u64 {
    xxhash_rust::xxh3::xxh3_64(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use islefall_sim::Cell;

    #[test]
    fn frames_round_trip() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            let (a, b) = tokio::io::duplex(4096);
            let (mut w, mut r) = (writer(a), reader(b));
            let msg = ServerMsg::Turn { turn: 7, commands: vec![(1, Command::Move { unit: 2, to: Cell::new(3, 4) })] };
            send(&mut w, &msg).await.unwrap();
            send(&mut w, &ClientMsg::Ping(9)).await.unwrap();
            let back: ServerMsg = recv(&mut r).await.unwrap();
            assert_eq!(back, msg);
            let ping: ClientMsg = recv(&mut r).await.unwrap();
            assert_eq!(ping, ClientMsg::Ping(9));
            drop(w);
            assert!(recv::<_, ClientMsg>(&mut r).await.is_err(), "a closed stream is an error, not a message");
        });
        // The wire layout: a little-endian u32 length, then the postcard bytes.
        let body = encode(&ClientMsg::Ping(9)).unwrap();
        let mut framed = Vec::new();
        framed.extend_from_slice(&(body.len() as u32).to_le_bytes());
        framed.extend_from_slice(&body);
        let mut c = codec();
        let mut buf = bytes::BytesMut::from(&framed[..]);
        let got = tokio_util::codec::Decoder::decode(&mut c, &mut buf).unwrap().unwrap();
        assert_eq!(&got[..], &body[..]);
        assert_ne!(data_hash(b"a"), data_hash(b"b"));
    }
}
