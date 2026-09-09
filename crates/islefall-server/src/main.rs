// SPDX-License-Identifier: Apache-2.0
//! The relay server: takes commands from every client, stamps them into
//! turns on a fixed clock, and sends each turn to everyone. It runs no
//! simulation of its own yet; clients report their world hashes and the
//! server tells them when they disagree.
//!
//! ```text
//! islefall-server [server.toml]
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use islefall_net::{ClientMsg, PROTOCOL, PlayerInfo, ServerMsg};
use serde::Deserialize;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, mpsc};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct ServerConfig {
    bind: String,
    tick_hz: u32,
    turn_ticks: u32,
    /// The game starts once this many players are ready.
    min_players: usize,
    max_players: usize,
    hash_every_turns: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig { bind: "0.0.0.0:7777".into(), tick_hz: 30, turn_ticks: 3, min_players: 2, max_players: 4, hash_every_turns: 100 }
    }
}

struct Player {
    info: PlayerInfo,
    tx: mpsc::UnboundedSender<ServerMsg>,
    data_hash: u64,
    map: String,
}

#[derive(Default)]
struct Game {
    players: Vec<Player>,
    started: bool,
    turn: u64,
    pending: Vec<(u8, islefall_net::Command)>,
    /// Hashes reported per turn, by player.
    hashes: BTreeMap<u64, BTreeMap<u8, u64>>,
    next_id: u8,
}

impl Game {
    fn broadcast(&self, msg: &ServerMsg) {
        for p in &self.players {
            let _ = p.tx.send(msg.clone());
        }
    }

    fn lobby(&self) -> ServerMsg {
        ServerMsg::Lobby { players: self.players.iter().map(|p| p.info.clone()).collect() }
    }
}

#[tokio::main]
async fn main() {
    let cfg: ServerConfig = match std::env::args().nth(1) {
        Some(path) => match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| toml::from_str(&t).map_err(|e| e.to_string())) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("islefall-server: {path}: {e}");
                std::process::exit(1);
            }
        },
        None => ServerConfig::default(),
    };
    let listener = match TcpListener::bind(&cfg.bind).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("islefall-server: cannot listen on {}: {e}", cfg.bind);
            std::process::exit(1);
        }
    };
    println!("islefall-server: listening on {}; {} ticks per turn at {} Hz; {}-{} players", cfg.bind, cfg.turn_ticks, cfg.tick_hz, cfg.min_players, cfg.max_players);
    let game = Arc::new(Mutex::new(Game::default()));
    tokio::spawn(turn_clock(game.clone(), cfg.clone()));
    loop {
        let (stream, addr) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                eprintln!("accept: {e}");
                continue;
            }
        };
        println!("connection from {addr}");
        tokio::spawn(serve(stream, game.clone(), cfg.clone()));
    }
}

/// Every turn: take the commands given since the last one and send them out.
async fn turn_clock(game: Arc<Mutex<Game>>, cfg: ServerConfig) {
    let period = Duration::from_secs_f64(cfg.turn_ticks.max(1) as f64 / cfg.tick_hz.max(1) as f64);
    let mut ticker = tokio::time::interval(period);
    loop {
        ticker.tick().await;
        let mut g = game.lock().await;
        if !g.started {
            continue;
        }
        let turn = g.turn;
        let commands = std::mem::take(&mut g.pending);
        g.broadcast(&ServerMsg::Turn { turn, commands });
        g.turn += 1;
    }
}

async fn serve(stream: TcpStream, game: Arc<Mutex<Game>>, cfg: ServerConfig) {
    let (mut reader, mut writer) = stream.into_split();
    let hello: ClientMsg = match islefall_net::recv(&mut reader).await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("hello: {e}");
            return;
        }
    };
    let ClientMsg::Hello { protocol, name, map, data_hash } = hello else {
        let _ = islefall_net::send(&mut writer, &ServerMsg::Reject("say hello first".into())).await;
        return;
    };
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMsg>();
    let id = {
        let mut g = game.lock().await;
        let refusal = if protocol != PROTOCOL {
            Some(format!("protocol {protocol}, this server speaks {PROTOCOL}"))
        } else if g.started {
            Some("the game has started".into())
        } else if g.players.len() >= cfg.max_players {
            Some("the game is full".into())
        } else if let Some(other) = g.players.first().filter(|p| p.data_hash != data_hash || p.map != map) {
            Some(format!("your rules or map differ from {}'s", other.info.name))
        } else {
            None
        };
        if let Some(why) = refusal {
            drop(g);
            let _ = islefall_net::send(&mut writer, &ServerMsg::Reject(why)).await;
            return;
        }
        let id = g.next_id;
        g.next_id += 1;
        g.players.push(Player { info: PlayerInfo { id, name: name.clone(), ready: false }, tx, data_hash, map });
        let _ = islefall_net::send(&mut writer, &ServerMsg::Welcome { player: id, turn_ticks: cfg.turn_ticks, hash_every_turns: cfg.hash_every_turns }).await;
        let lobby = g.lobby();
        g.broadcast(&lobby);
        id
    };
    println!("player {id} is {name}");
    let write_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if islefall_net::send(&mut writer, &msg).await.is_err() {
                break;
            }
        }
    });
    loop {
        let msg: ClientMsg = match islefall_net::recv(&mut reader).await {
            Ok(m) => m,
            Err(_) => break,
        };
        let mut g = game.lock().await;
        match msg {
            ClientMsg::Hello { .. } => {}
            ClientMsg::Ready(ready) => {
                if let Some(p) = g.players.iter_mut().find(|p| p.info.id == id) {
                    p.info.ready = ready;
                }
                let lobby = g.lobby();
                g.broadcast(&lobby);
                if !g.started && g.players.len() >= cfg.min_players && g.players.iter().all(|p| p.info.ready) {
                    let map = g.players[0].map.clone();
                    println!("starting on {map} with {} players", g.players.len());
                    g.started = true;
                    g.broadcast(&ServerMsg::Start { map });
                }
            }
            ClientMsg::Command(cmd) => {
                if g.started {
                    g.pending.push((id, cmd));
                }
            }
            ClientMsg::Hash { turn, hash } => {
                g.hashes.entry(turn).or_default().insert(id, hash);
                let count = g.players.len();
                let complete = g.hashes.get(&turn).is_some_and(|h| h.len() >= count);
                if complete {
                    let hashes: Vec<(u8, u64)> = g.hashes.remove(&turn).unwrap_or_default().into_iter().collect();
                    let agree = hashes.windows(2).all(|w| w[0].1 == w[1].1);
                    if agree {
                        println!("turn {turn}: {} players agree on {:016x}", hashes.len(), hashes[0].1);
                    } else {
                        println!("desync at turn {turn}: {hashes:?}");
                        g.broadcast(&ServerMsg::Desync { turn, hashes });
                    }
                    g.hashes.retain(|&t, _| t > turn);
                }
            }
            ClientMsg::Ping(n) => {
                if let Some(p) = g.players.iter().find(|p| p.info.id == id) {
                    let _ = p.tx.send(ServerMsg::Pong(n));
                }
            }
        }
    }
    let mut g = game.lock().await;
    g.players.retain(|p| p.info.id != id);
    g.broadcast(&ServerMsg::Left(id));
    let lobby = g.lobby();
    g.broadcast(&lobby);
    println!("player {id} left");
    write_task.abort();
}
