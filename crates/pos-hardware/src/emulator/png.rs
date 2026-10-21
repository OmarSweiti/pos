//! A printed page as a PNG: one bit per dot, the image a manual pass reads.
//!
//! The bitmap stores 1 for a black dot; a 1-bit grayscale PNG stores 1 for
//! white, so every byte is inverted on the way out and on the way back in.

use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::Path;

use crate::render::Bitmap;

fn other(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::other(error)
}

/// Write `page` to `path` as a 1-bit grayscale PNG.
///
/// # Errors
///
/// The file cannot be created, or the encoder refuses the page.
pub fn write(path: &Path, page: &Bitmap) -> io::Result<()> {
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(path)?),
        page.width_px,
        page.height_px,
    );
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::One);
    let mut writer = encoder.write_header().map_err(other)?;
    let inverted: Vec<u8> = page.bits.iter().map(|byte| !byte).collect();
    writer.write_image_data(&inverted).map_err(other)?;
    writer.finish().map_err(other)
}

/// Read a page [`write`] wrote back into a bitmap.
///
/// # Errors
///
/// The file cannot be read, or is not a 1-bit grayscale PNG.
pub fn read(path: &Path) -> io::Result<Bitmap> {
    let decoder = png::Decoder::new(BufReader::new(File::open(path)?));
    let mut reader = decoder.read_info().map_err(other)?;
    let info = reader.info();
    if info.color_type != png::ColorType::Grayscale || info.bit_depth != png::BitDepth::One {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a 1-bit grayscale PNG",
        ));
    }
    let (width_px, height_px) = (info.width, info.height);
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "a page too large to read"))?;
    let mut buffer = vec![0; size];
    let frame = reader.next_frame(&mut buffer).map_err(other)?;
    buffer.truncate(frame.buffer_size());
    Ok(Bitmap {
        width_px,
        height_px,
        bits: buffer.iter().map(|byte| !byte).collect(),
    })
}
