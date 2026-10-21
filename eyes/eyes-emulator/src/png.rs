//! A dependency-free PNG writer: 8-bit RGB, deflate "stored" blocks (no compression). Contact sheets are
//! small enough that size doesn't matter, and this keeps the emulator offline-buildable.

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
const MAX_STORED_BLOCK: usize = 65_535;

/// Encode `rgb` (row-major, 3 bytes per pixel) as a PNG.
pub fn encode_rgb(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    assert_eq!(
        rgb.len(),
        width as usize * height as usize * 3,
        "pixel buffer size"
    );
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    // 8-bit depth, colour type 2 (RGB), deflate, adaptive filtering, no interlace.
    header.extend_from_slice(&[8, 2, 0, 0, 0]);

    // Every scanline is prefixed with filter type 0 (none).
    let scanlines: Vec<u8> = rgb
        .chunks_exact(width as usize * 3)
        .flat_map(|row| std::iter::once(0).chain(row.iter().copied()))
        .collect();

    let mut png = SIGNATURE.to_vec();
    write_chunk(&mut png, b"IHDR", &header);
    write_chunk(&mut png, b"IDAT", &zlib_stored(&scanlines));
    write_chunk(&mut png, b"IEND", &[]);
    png
}

fn write_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = png.len();
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    let crc = crc32(&png[start..]);
    png.extend_from_slice(&crc.to_be_bytes());
}

fn zlib_stored(data: &[u8]) -> Vec<u8> {
    // CMF/FLG: deflate with a 32 KiB window, no dictionary, check bits valid.
    let mut out = vec![0x78, 0x01];
    let blocks = data.chunks(MAX_STORED_BLOCK);
    let count = blocks.len().max(1);
    if data.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    for (index, block) in blocks.enumerate() {
        let is_final = u8::from(index + 1 == count);
        let length = block.len() as u16;
        out.push(is_final);
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&(!length).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0u32, |crc, &byte| {
        (0..8).fold(crc ^ u32::from(byte), |crc, _| {
            if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            }
        })
    })
}

fn adler32(bytes: &[u8]) -> u32 {
    let (a, b) = bytes.iter().fold((1u32, 0u32), |(a, b), &byte| {
        let a = (a + u32::from(byte)) % 65_521;
        (a, (b + a) % 65_521)
    });
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_reference_values() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn png_has_signature_header_and_trailer() {
        let png = encode_rgb(2, 1, &[255, 0, 0, 0, 255, 0]);
        let ihdr_dimensions = &png[16..24];
        let ok = png.starts_with(&SIGNATURE)
            && ihdr_dimensions == [0, 0, 0, 2, 0, 0, 0, 1]
            && png.ends_with(&[b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]);
        assert!(ok, "{png:02X?}");
    }

    #[test]
    fn large_images_split_into_multiple_stored_blocks() {
        let data = vec![7u8; MAX_STORED_BLOCK + 10];
        let zlib = zlib_stored(&data);
        // Two block headers (5 bytes each), zlib header (2) and Adler-32 (4).
        assert_eq!(zlib.len(), data.len() + 2 * 5 + 2 + 4);
    }
}
