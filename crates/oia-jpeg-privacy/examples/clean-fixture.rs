use oia_jpeg_privacy::{Limits, Selection, clean};
use std::error::Error;
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn Error>> {
    // Synthetic fixtures only: no argument selects baseline.hex, `progressive` selects progressive.hex.
    let hex = match std::env::args().nth(1).as_deref() {
        None => include_str!("../tests/fixtures/baseline.hex"),
        Some("progressive") => include_str!("../tests/fixtures/progressive.hex"),
        Some(_) => return Err("usage: clean-fixture [progressive]".into()),
    }
    .trim();
    if !hex.len().is_multiple_of(2) {
        return Err("synthetic fixture has an odd hex length".into());
    }
    let input: Vec<u8> = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect::<Result<_, Box<dyn Error>>>()?;
    let cleaned = clean(
        &input,
        Selection {
            comment: true,
            exif: true,
            xmp: true,
            photoshop_irb: true,
        },
        Limits {
            input_bytes: 4096,
            output_bytes: 4096,
            segments: 64,
            metadata_bytes: 1024,
            width: 16,
            height: 16,
            pixels: 256,
        },
        || Ok(()),
    )?;
    io::stdout().lock().write_all(cleaned.bytes())?;
    Ok(())
}
