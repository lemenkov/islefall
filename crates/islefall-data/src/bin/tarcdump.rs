// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! Inspect a NetStorm `netstorm.tarc` archive.
//!
//! `tarcdump --help` lists the commands: `list`, `cat`, `extract`, `types`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use islefall_data::tarc::Archive;
use islefall_data::typefile;

/// Inspect a NetStorm `netstorm.tarc` archive.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the entries.
    List { archive: PathBuf },
    /// Print an entry as text.
    Cat { archive: PathBuf, name: String },
    /// Write an entry's bytes to a file.
    Extract { archive: PathBuf, name: String, out: PathBuf },
    /// Summarise every type definition.
    Types { archive: PathBuf },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tarcdump: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::List { archive } => {
            let a = Archive::load(&archive)?;
            for e in a.entries() {
                println!("{:>8}  {}", e.size, e.name);
            }
            println!("{} files", a.entries().len());
            Ok(())
        }
        Command::Cat { archive, name } => {
            let a = Archive::load(&archive)?;
            let i = a.find(&name).ok_or_else(|| format!("{name} not in archive"))?;
            print!("{}", a.read_text(i));
            Ok(())
        }
        Command::Extract { archive, name, out } => {
            let a = Archive::load(&archive)?;
            let i = a.find(&name).ok_or_else(|| format!("{name} not in archive"))?;
            std::fs::write(&out, a.read(i))?;
            Ok(())
        }
        Command::Types { archive } => {
            let a = Archive::load(&archive)?;
            let mut bad = 0;
            for (i, e) in a.entries().iter().enumerate() {
                if e.extension() != "type" {
                    continue;
                }
                match typefile::parse(&a.read_text(i)) {
                    Ok(t) => println!(
                        "{:<28} {:<24} theme={:<8} hp={:<5} frames={:<4} anims={:<3} images={:?}",
                        e.basename(),
                        t.get_str("description").unwrap_or("-"),
                        t.get_str("theme").unwrap_or("-"),
                        t.get_i64("maxHitPoints").map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
                        t.frames.len(),
                        t.animations().len(),
                        t.image_files(),
                    ),
                    Err(err) => {
                        bad += 1;
                        println!("{:<28} PARSE ERROR {err}", e.basename());
                    }
                }
            }
            if bad > 0 {
                return Err(format!("{bad} type files failed to parse").into());
            }
            Ok(())
        }
    }
}
