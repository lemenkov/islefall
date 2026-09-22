// SPDX-FileCopyrightText: 2026 Peter Lemenkov <lemenkov@gmail.com>
// SPDX-License-Identifier: Apache-2.0

//! Write generated art to PNG files, to look at it or to ship it as a sheet.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use islefall_art::Picture;
use islefall_art::fire::{Blast, Flame, Smoke, strip};
use islefall_art::ground::Ground;
use islefall_art::islet::Islet;
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
    /// A unit's islet: the top over the rock under it.
    Islet {
        #[arg(long, default_value_t = 48)]
        width: u32,
        #[arg(long, default_value_t = 33)]
        height: u32,
        #[arg(long, default_value_t = 7)]
        seed: u32,
        out: PathBuf,
    },
    /// The frames of a looping flame, side by side.
    Flame {
        #[arg(long, default_value_t = 12)]
        width: u32,
        #[arg(long, default_value_t = 20)]
        height: u32,
        #[arg(long, default_value_t = 7)]
        seed: u32,
        out: PathBuf,
    },
    /// The frames of a puff of smoke, side by side.
    Smoke {
        #[arg(long, default_value_t = 14)]
        size: u32,
        #[arg(long, default_value_t = 7)]
        seed: u32,
        out: PathBuf,
    },
    /// The frames of an explosion, side by side.
    Blast {
        #[arg(long, default_value_t = 64)]
        size: u32,
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
        Command::Islet { width, height, seed, out } => {
            let islet = Islet {
                ground: Ground { ramp: vec![[52, 78, 30], [70, 102, 38], [88, 126, 48], [110, 150, 60]], ..Ground::default() },
                rock: Rock { windows_per_100px: 0.0, lit: false, depth_min: 20, depth_max: 38, ..Rock::default() },
                light: [120, 170, 240],
                dark: [50, 80, 140],
                width,
                height,
                wobble: 0.2,
                band: 3,
            };
            let (top, under) = islet.pictures(seed);
            let mut both = Picture::new(top.width, under.height);
            for y in 0..under.height {
                for x in 0..top.width {
                    let i = ((y * top.width + x) * 4) as usize;
                    if under.rgba[i + 3] > 0 {
                        both.put(x, y, [under.rgba[i], under.rgba[i + 1], under.rgba[i + 2]]);
                    }
                    if y < top.height && top.alpha(x, y) > 0 {
                        both.put(x, y, [top.rgba[i], top.rgba[i + 1], top.rgba[i + 2]]);
                    }
                }
            }
            write_png(&both, &out)
        }
        Command::Flame { width, height, seed, out } => write_png(&strip(&Flame { width, height, ..Flame::default() }.frames(seed), 0), &out),
        Command::Smoke { size, seed, out } => write_png(&strip(&Smoke { size, ..Smoke::default() }.frames(seed), 0), &out),
        Command::Blast { size, seed, out } => write_png(&strip(&Blast { size, ..Blast::default() }.frames(seed), 0), &out),
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
