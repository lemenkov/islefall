// SPDX-License-Identifier: Apache-2.0
//! Inspect a NetStorm `netstorm.tarc` archive.
//!
//! ```text
//! tarcdump list  <netstorm.tarc>
//! tarcdump cat   <netstorm.tarc> <name>
//! tarcdump types <netstorm.tarc>
//! ```

use std::process::ExitCode;

use islefall_data::tarc::Archive;
use islefall_data::typefile;

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tarcdump: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    match args.first().map(String::as_str) {
        Some("list") if args.len() == 2 => {
            let a = Archive::load(&args[1])?;
            for e in a.entries() {
                println!("{:>8}  {}", e.size, e.name);
            }
            println!("{} files", a.entries().len());
            Ok(())
        }
        Some("cat") if args.len() == 3 => {
            let a = Archive::load(&args[1])?;
            let i = a.find(&args[2]).ok_or_else(|| format!("{} not in archive", args[2]))?;
            print!("{}", a.read_text(i));
            Ok(())
        }
        Some("types") if args.len() == 2 => {
            let a = Archive::load(&args[1])?;
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
        _ => {
            eprintln!("usage:\n  tarcdump list <netstorm.tarc>\n  tarcdump cat <netstorm.tarc> <name>\n  tarcdump types <netstorm.tarc>");
            Err("bad arguments".into())
        }
    }
}
