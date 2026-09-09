// SPDX-License-Identifier: Apache-2.0
//! Inspect a NetStorm `_shapes.shp`: print statistics or render a container
//! to a PNG sprite sheet.
//!
//! ```text
//! shpdump stats <_shapes.shp>
//! shpdump sheet <_shapes.shp> <container> <out.png> [--col FILE.COL] [--scale N] [--max N]
//! ```

use std::process::ExitCode;

use islefall_data::shp::{MAX_DIM, MAX_PIXELS};
use islefall_data::{Frame, Palette, ShapeFile};

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("shpdump: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    match args.first().map(String::as_str) {
        Some("stats") if args.len() == 2 => stats(&args[1]),
        Some("sheet") if args.len() >= 4 => sheet(&args[1..]),
        _ => {
            eprintln!("usage:\n  shpdump stats <_shapes.shp>\n  shpdump sheet <_shapes.shp> <container> <out.png> [--col FILE.COL] [--scale N] [--max N]");
            Err("bad arguments".into())
        }
    }
}

fn stats(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let shp = ShapeFile::load(path)?;
    let (mut ok, mut empty, mut nofit, mut bad) = (0usize, 0usize, 0usize, 0usize);
    println!("{} containers, {} unique records", shp.container_count(), shp.records().len());
    for ci in 0..shp.container_count() {
        let mut errors = Vec::new();
        let mut sizes = Vec::new();
        for (ei, r) in shp.decode_container(ci).into_iter().enumerate() {
            match r {
                Ok(fr) if fr.is_empty() => {
                    ok += 1;
                    empty += 1;
                }
                Ok(fr) => {
                    ok += 1;
                    if !fr.fits_canvas() {
                        nofit += 1;
                    }
                    sizes.push((fr.width, fr.height));
                }
                Err(e) => {
                    bad += 1;
                    errors.push(format!("[{ei}] {e}"));
                }
            }
        }
        let canvas = shp.container(ci).first().and_then(|&o| shp.decode(o).ok()).map(|f| f.header.canvas_size());
        let (mw, mh) = sizes.iter().fold((0, 0), |(w, h), &(fw, fh)| (w.max(fw), h.max(fh)));
        println!(
            "c{ci:<3} entries={:<4} canvas={:?} max_block={mw}x{mh} errors={}",
            shp.container(ci).len(),
            canvas,
            errors.len()
        );
        for e in errors.iter().take(3) {
            println!("      {e}");
        }
    }
    println!("ok={ok} (empty={empty}, not fitting canvas={nofit}) bad={bad}");
    Ok(())
}

fn sheet(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = &args[0];
    let ci: usize = args[1].parse()?;
    let out = &args[2];
    let mut col: Option<String> = None;
    let mut scale = 2usize;
    let mut max = 40usize;
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--col" => {
                col = Some(args.get(i + 1).ok_or("--col needs a file")?.clone());
                i += 2;
            }
            "--scale" => {
                scale = args.get(i + 1).ok_or("--scale needs a number")?.parse()?;
                i += 2;
            }
            "--max" => {
                max = args.get(i + 1).ok_or("--max needs a number")?.parse()?;
                i += 2;
            }
            other => return Err(format!("unknown option {other}").into()),
        }
    }
    if scale == 0 || scale > 8 {
        return Err("scale must be 1..8".into());
    }
    let shp = ShapeFile::load(path)?;
    if ci >= shp.container_count() {
        return Err(format!("container {ci} out of range").into());
    }
    let pal = match col {
        Some(c) => Palette::load(c)?,
        None => Palette::grey(),
    };

    let mut frames = Vec::new();
    for (ei, r) in shp.decode_container(ci).into_iter().enumerate().take(max) {
        match r {
            Ok(fr) => frames.push(fr),
            Err(e) => eprintln!("c{ci}[{ei}]: {e}"),
        }
    }
    if frames.is_empty() {
        return Err("nothing decoded".into());
    }

    // Each frame is drawn on its own canvas (grown if the block does not fit).
    let cells: Vec<(usize, usize)> = frames.iter().map(cell_size).collect();
    let gap = 2;
    let mut cols = frames.len();
    let layout = |cols: usize| -> (usize, usize) {
        let mut w = 0;
        let mut h = 0;
        for row in cells.chunks(cols) {
            w = w.max(row.iter().map(|c| c.0 + gap).sum::<usize>());
            h += row.iter().map(|c| c.1).max().unwrap_or(0) + gap;
        }
        (w, h)
    };
    let (mut w, mut h) = layout(cols);
    while cols > 1 && (w * scale > MAX_DIM || h * scale > MAX_DIM || w * h * scale * scale > MAX_PIXELS) {
        cols = cols.div_ceil(2);
        (w, h) = layout(cols);
    }
    if w * scale > MAX_DIM || h * scale > MAX_DIM || w * h * scale * scale > MAX_PIXELS {
        return Err(format!("sheet {w}x{h} at scale {scale} exceeds limits").into());
    }

    let (sw, sh) = (w * scale, h * scale);
    let mut rgba = vec![0u8; sw * sh * 4];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&[40, 40, 40, 255]);
    }
    let mut x0 = 0;
    let mut y0 = 0;
    let mut row_h = 0;
    for (i, (fr, &(cw, ch))) in frames.iter().zip(&cells).enumerate() {
        if i > 0 && i % cols == 0 {
            x0 = 0;
            y0 += row_h + gap;
            row_h = 0;
        }
        draw(&mut rgba, sw, fr, &pal, x0 * scale, y0 * scale, scale);
        x0 += cw + gap;
        row_h = row_h.max(ch);
    }

    let file = std::fs::File::create(out)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), sw as u32, sh as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgba)?;
    println!("wrote {out}: {} frames, {sw}x{sh}", frames.len());
    Ok(())
}

/// Canvas size for one frame, grown to include a block that does not fit.
fn cell_size(fr: &Frame) -> (usize, usize) {
    let (cw, ch) = fr.header.canvas_size();
    if fr.is_empty() {
        return (cw, ch);
    }
    let right = (fr.block_x.max(0) as usize + fr.width).max(cw);
    let bottom = (fr.block_y.max(0) as usize + fr.height).max(ch);
    (right, bottom)
}

fn draw(rgba: &mut [u8], sw: usize, fr: &Frame, pal: &Palette, x0: usize, y0: usize, scale: usize) {
    for y in 0..fr.height {
        for x in 0..fr.width {
            let Some(idx) = fr.pixel(x, y) else { continue };
            let [r, g, b] = pal.rgb(idx);
            // Negative block offsets are clamped to the canvas origin.
            let cx = x0 + (fr.block_x.max(0) as usize + x) * scale;
            let cy = y0 + (fr.block_y.max(0) as usize + y) * scale;
            for dy in 0..scale {
                for dx in 0..scale {
                    let o = ((cy + dy) * sw + cx + dx) * 4;
                    rgba[o..o + 4].copy_from_slice(&[r, g, b, 255]);
                }
            }
        }
    }
}
