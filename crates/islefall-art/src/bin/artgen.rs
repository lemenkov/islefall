// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! Write generated art to PNG files, to look at it or to ship it as a sheet.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use islefall_art::Picture;
use islefall_art::ground::Ground;
use islefall_art::rock::Rock;

/// Make Islefall's generated art and write it as PNG
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The rock under a run of island edge.
    Underside {
        /// Width of the run in source pixels (16 to a cell).
        #[arg(long, default_value_t = 384)]
        width: u32,
        #[arg(long, default_value_t = 7)]
        seed: u32,
        /// Dark windows, as on an island nobody owns.
        #[arg(long)]
        unlit: bool,
        out: PathBuf,
    },
    /// A rectangle of island surface.
    Ground {
        #[arg(long, default_value_t = 384)]
        width: u32,
        #[arg(long, default_value_t = 132)]
        height: u32,
        #[arg(long, default_value_t = 7)]
        seed: u32,
        out: PathBuf,
    },
}

fn write_png(pic: &Picture, path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, pic.width, pic.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&pic.rgba)?;
    Ok(())
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Underside { width, seed, unlit, out } => {
            let rock = Rock { lit: !unlit, ..Rock::default() };
            write_png(&rock.underside(width.max(1), seed), &out)
        }
        Command::Ground { width, height, seed, out } => write_png(&Ground::default().surface(width.max(1), height.max(1), seed, |_, _| true), &out),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("artgen: {e}");
            ExitCode::FAILURE
        }
    }
}
