// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! Show a campaign scenario from `netstorm.tarc`.
//!
//! `fortdump --help` lists the commands: `list`, `show <name>` (e.g. `capturethepriest`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use islefall_data::fort::{self, Fort};
use islefall_data::mission::Mission;
use islefall_data::tarc::Archive;

/// Show the campaign scenarios in a NetStorm `netstorm.tarc` archive.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the scenarios and how they parse.
    List { archive: PathBuf },
    /// Draw a scenario's world table and list its fortresses.
    Show { archive: PathBuf, name: String },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fortdump: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::List { archive } => {
            let a = Archive::load(&archive)?;
            for (i, e) in a.entries().iter().enumerate() {
                if e.extension().eq_ignore_ascii_case("fort") {
                    match Fort::parse(&a.read(i)) {
                        Ok(f) => println!("{:<22} v{:<3} {:>4} items {:>2} fortresses{}", e.basename(), f.version, f.items.len(), f.fortresses.len(), if f.skipped_chunks > 0 { format!("  ({} chunks skipped)", f.skipped_chunks) } else { String::new() }),
                        Err(err) => println!("{:<22} ERROR {err}", e.basename()),
                    }
                }
            }
            Ok(())
        }
        Command::Show { archive, name } => {
            let a = Archive::load(&archive)?;
            let name = name.trim_end_matches(".fort");
            let i = a.find(&format!("{name}.fort")).ok_or_else(|| format!("{name}.fort not in archive"))?;
            let f = Fort::parse(&a.read(i))?;
            println!("{name}: version {} name {:?} {} items {} fortresses", f.version, f.name, f.items.len(), f.fortresses.len());
            if let Some(m) = Mission::for_fort(&a, name) {
                for (k, v) in &m.header {
                    if k.contains("tech") || k.contains("money") || k == "title" || k == "moregeysers" {
                        println!("  {k} = {v}");
                    }
                }
            }
            if let Some((x0, y0, x1, y1)) = f.bounds() {
                println!("world table spans x {x0}..{x1} y {y0}..{y1}");
                let mut grid: BTreeMap<(i32, i32), char> = BTreeMap::new();
                for it in &f.items {
                    let ch = match it.code {
                        fort::CODE_GROUND => '#',
                        fort::CODE_GEYSER => 'G',
                        c if fort::CODE_BRIDGES.contains(&c) => '=',
                        _ => '?',
                    };
                    let e = grid.entry((it.x, it.y)).or_insert(ch);
                    if *e == '#' || ch == 'G' {
                        *e = ch;
                    }
                }
                for y in y0..=y1 {
                    let row: String = (x0..=x1).map(|x| grid.get(&(x, y)).copied().unwrap_or('.')).collect();
                    println!("{y:4} {row}");
                }
            }
            let mut codes: BTreeMap<u8, usize> = BTreeMap::new();
            for it in &f.items {
                if it.code != fort::CODE_GROUND {
                    *codes.entry(it.code).or_default() += 1;
                }
            }
            println!("world table codes: {}", codes.iter().map(|(c, n)| format!("{c:02x}x{n}")).collect::<Vec<_>>().join(" "));
            for (k, fo) in f.fortresses.iter().enumerate() {
                let (w, h) = fo.canvas_size();
                println!("fortress {k}: canvas {w}x{h} ({} blocks)", fo.blocks);
                for it in &fo.items {
                    println!("   ({:2},{:2}) {:02x} {:<10} owner {:?}{}", it.x, it.y, it.code, hex(&it.payload), it.owner(), if it.is_local_temple() { "  (local seat)" } else { "" });
                }
            }
            if f.skipped_chunks > 0 {
                println!("{} record chunks skipped", f.skipped_chunks);
            }
            Ok(())
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
