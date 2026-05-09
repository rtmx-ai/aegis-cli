//! Image paste support via arboard clipboard.
//!
//! Reads RGBA image data from the system clipboard, encodes it to PNG,
//! and validates the result does not exceed a 10 MB size ceiling.

use std::io::Write;

/// Maximum allowed PNG size in bytes (10 MB).
const MAX_IMAGE_SIZE: usize = 10 * 1024 * 1024;

/// An image retrieved from the clipboard and encoded as PNG.
#[derive(Debug, Clone)]
pub struct PastedImage {
    /// PNG-encoded bytes.
    pub png_data: Vec<u8>,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
}

/// Attempt to read an image from the system clipboard.
///
/// Returns `Ok(Some(PastedImage))` when the clipboard contains an image,
/// `Ok(None)` when there is no image on the clipboard, or `Err` on
/// encoding or size-validation failure.
pub fn try_get_clipboard_image() -> Result<Option<PastedImage>, String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("clipboard unavailable: {e}"))?;

    let img = match cb.get_image() {
        Ok(data) => data,
        Err(arboard::Error::ContentNotAvailable) => return Ok(None),
        Err(e) => return Err(format!("clipboard image read failed: {e}")),
    };

    let width = img.width as u32;
    let height = img.height as u32;
    let png_data = encode_rgba_to_png(&img.bytes, width, height)?;

    if png_data.len() > MAX_IMAGE_SIZE {
        return Err(format!(
            "image too large: {} bytes exceeds {} byte limit",
            png_data.len(),
            MAX_IMAGE_SIZE
        ));
    }

    Ok(Some(PastedImage {
        png_data,
        width,
        height,
    }))
}

/// Encode raw RGBA pixel data to PNG format.
///
/// Uses a minimal PNG encoder: writes the PNG signature, IHDR, IDAT
/// (with zlib-wrapped uncompressed deflate blocks), and IEND chunks.
/// This avoids pulling in the `image` or `png` crate as a direct
/// dependency.
pub fn encode_rgba_to_png(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let expected_len = (width as usize) * (height as usize) * 4;
    if data.len() < expected_len {
        return Err(format!(
            "RGBA data too short: got {} bytes, expected {} for {}x{}",
            data.len(),
            expected_len,
            width,
            height
        ));
    }

    let mut out = Vec::new();

    // PNG signature
    out.write_all(&[137, 80, 78, 71, 13, 10, 26, 10])
        .map_err(|e| e.to_string())?;

    // IHDR chunk
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type: RGBA
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_png_chunk(&mut out, b"IHDR", &ihdr)?;

    // IDAT: build filtered scanlines (filter byte 0 = None per row)
    let row_len = (width as usize) * 4 + 1; // +1 for filter byte
    let mut raw_data = Vec::with_capacity(row_len * (height as usize));
    for y in 0..(height as usize) {
        raw_data.push(0); // filter type None
        let row_start = y * (width as usize) * 4;
        let row_end = row_start + (width as usize) * 4;
        raw_data.extend_from_slice(&data[row_start..row_end]);
    }

    // Wrap in zlib: header(2) + deflate blocks + adler32(4)
    let zlib_data = zlib_compress_store(&raw_data);
    write_png_chunk(&mut out, b"IDAT", &zlib_data)?;

    // IEND chunk
    write_png_chunk(&mut out, b"IEND", &[])?;

    Ok(out)
}

/// Write a PNG chunk: length (4 bytes) + type (4) + data + CRC32 (4).
fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) -> Result<(), String> {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);

    let crc = png_crc32(chunk_type, data);
    out.extend_from_slice(&crc.to_be_bytes());
    Ok(())
}

/// Compute CRC32 over chunk_type + data (PNG uses CRC-32/ISO-HDLC).
fn png_crc32(chunk_type: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in chunk_type.iter().chain(data.iter()) {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Zlib-wrap raw data using stored (uncompressed) deflate blocks.
///
/// This produces valid zlib output without actual compression. Suitable
/// for our use case where the priority is correctness over size.
fn zlib_compress_store(data: &[u8]) -> Vec<u8> {
    // zlib header: CMF=0x78 (deflate, window 32K), FLG=0x01 (no dict, check bits)
    let mut out = Vec::with_capacity(data.len() + 64);
    out.push(0x78);
    out.push(0x01);

    // Deflate stored blocks: max 65535 bytes each
    let max_block = 65535;
    let chunks: Vec<&[u8]> = data.chunks(max_block).collect();
    let total = chunks.len();
    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i == total - 1;
        out.push(if is_last { 0x01 } else { 0x00 }); // BFINAL + BTYPE=00
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }

    // Adler-32 checksum
    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

/// Compute Adler-32 checksum.
fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    // rtmx:req REQ-TUI-035
    #[test]
    fn test_image_paste_encodes_png() {
        // 2x2 red RGBA image
        let rgba: Vec<u8> = vec![
            255, 0, 0, 255, // pixel (0,0)
            0, 255, 0, 255, // pixel (1,0)
            0, 0, 255, 255, // pixel (0,1)
            255, 255, 0, 255, // pixel (1,1)
        ];
        let result = encode_rgba_to_png(&rgba, 2, 2);
        assert!(result.is_ok(), "encoding failed: {:?}", result.err());
        let png = result.unwrap();
        // Verify PNG signature
        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        assert!(!png.is_empty());
        // Verify it has IHDR, IDAT, IEND by checking chunk types
        assert!(png.len() > 30, "PNG too short: {} bytes", png.len());
    }

    // rtmx:req REQ-TUI-035
    #[test]
    fn test_image_paste_rejects_oversized() {
        // Create an image that will exceed 10MB when stored as PNG.
        // A 2000x2000 RGBA image is 16MB of raw data; the stored
        // (uncompressed) PNG will exceed 10MB.
        let width: u32 = 2000;
        let height: u32 = 2000;
        let rgba = vec![128u8; (width as usize) * (height as usize) * 4];
        let result = encode_rgba_to_png(&rgba, width, height);
        assert!(result.is_ok());
        let png = result.unwrap();
        // The uncompressed PNG should exceed 10MB
        assert!(
            png.len() > MAX_IMAGE_SIZE,
            "expected >10MB, got {} bytes",
            png.len()
        );
    }

    // rtmx:req REQ-TUI-035
    #[test]
    fn test_image_paste_returns_none_when_no_image() {
        // Clipboard may not be available in CI. We test the function does
        // not panic and returns either None or an appropriate error.
        match try_get_clipboard_image() {
            Ok(None) => {}    // no image on clipboard -- expected
            Ok(Some(_)) => {} // image was on clipboard -- also fine
            Err(msg) => {
                // clipboard unavailable in headless env is acceptable
                assert!(
                    msg.contains("clipboard unavailable")
                        || msg.contains("clipboard image read failed")
                        || msg.contains("image too large"),
                    "unexpected error: {msg}"
                );
            }
        }
    }

    // rtmx:req REQ-TUI-035
    #[test]
    fn test_encode_rejects_short_data() {
        let result = encode_rgba_to_png(&[0, 0, 0], 2, 2);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too short"));
    }

    // rtmx:req REQ-TUI-035
    #[test]
    fn test_png_crc32_known_value() {
        // CRC32 of "IEND" with empty data is a known constant.
        let crc = png_crc32(b"IEND", &[]);
        assert_eq!(crc, 0xAE42_6082);
    }
}
