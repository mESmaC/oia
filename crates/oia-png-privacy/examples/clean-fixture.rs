use oia_png_privacy::{Limits, Selection, clean};
use std::error::Error;
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn Error>> {
    let hex = include_str!("../tests/fixtures/rgba.hex").trim();
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
            text: true,
            modification_time: true,
        },
        Limits {
            input_bytes: 1024,
            output_bytes: 1024,
            chunk_bytes: 128,
            chunks: 16,
            metadata_bytes: 256,
            width: 1,
            height: 1,
            pixels: 1,
        },
        || Ok(()),
    )?;
    io::stdout().lock().write_all(cleaned.bytes())?;
    Ok(())
}
