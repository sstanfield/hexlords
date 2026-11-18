//! A small PNG writer for screenshots: 8-bit RGB, no compression (stored deflate
//! blocks). It exists so that screenshots need no dependency; the files are
//! large but every PNG reader opens them.
//!
//! Not a port: the C++ had no screenshots.

/// Encode an image as a PNG file. `rgba` is `width * height * 4` bytes, top row
/// first; the alpha channel is dropped (the picture is what the screen shows).
/// `None` if the size and the data do not match, or a side is zero.
pub fn encode_rgb(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() != w.checked_mul(h)?.checked_mul(4)? {
        return None;
    }
    // The scanlines: a filter byte (0, none) and then the row's RGB.
    let mut raw = Vec::with_capacity(h * (1 + w * 3));
    for row in rgba.chunks_exact(w * 4) {
        raw.push(0);
        for px in row.chunks_exact(4) {
            raw.extend_from_slice(&px[..3]);
        }
    }

    let mut out = Vec::with_capacity(raw.len() + raw.len() / 65535 * 5 + 100);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bits, color type RGB, no interlace
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    Some(out)
}

/// A PNG chunk: length, type, data, CRC of type and data.
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(kind, data).to_be_bytes());
}

/// `data` as a zlib stream of stored (uncompressed) deflate blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65535 * 5 + 16);
    out.extend_from_slice(&[0x78, 0x01]); // deflate, 32K window, no preset dictionary
    let mut blocks = data.chunks(65535).peekable();
    if blocks.peek().is_none() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]); // one empty final block
    }
    while let Some(block) = blocks.next() {
        let len = block.len() as u16;
        out.push(u8::from(blocks.peek().is_none())); // the final-block bit; type 00 = stored
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    // 5552 bytes is the most that cannot overflow a u32 before the modulo.
    for part in data.chunks(5552) {
        for &byte in part {
            a += u32::from(byte);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

/// The CRC-32 of `kind` followed by `data` (the polynomial PNG and zlib use).
fn crc32(kind: &[u8], data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in kind.iter().chain(data) {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use turbo_ferret::image::{Limits, png};

    /// A picture with every byte value in it somewhere.
    fn picture(w: u32, h: u32) -> Vec<u8> {
        (0..w * h)
            .flat_map(|i| [(i % 256) as u8, (i / 7 % 256) as u8, (i * 31 % 256) as u8, (i % 5 * 50) as u8])
            .collect()
    }

    fn round_trip(w: u32, h: u32) {
        let rgba = picture(w, h);
        let file = encode_rgb(w, h, &rgba).unwrap();
        // Turbo Ferret's decoder checks the zlib Adler-32 and the structure.
        let back = png::decode(&file, &Limits::DEFAULT).unwrap_or_else(|e| panic!("{w}x{h}: {e}"));
        assert_eq!((back.size.w, back.size.h), (w as i32, h as i32));
        for (i, (got, want)) in back.rgba.chunks(4).zip(rgba.chunks(4)).enumerate() {
            assert_eq!(got[..3], want[..3], "{w}x{h}: pixel {i}");
            assert_eq!(got[3], 255, "{w}x{h}: the alpha is dropped");
        }
    }

    #[test]
    fn what_is_written_reads_back() {
        round_trip(1, 1);
        round_trip(3, 2);
        round_trip(64, 64);
        // More than one stored block (65535 bytes each), and exactly at a block's end.
        round_trip(300, 200);
        round_trip(1456, 15); // 15 * (1 + 1456 * 3) = 65535 bytes: one block exactly
        round_trip(1, 16384); // 65536 bytes: one byte into a second block
    }

    #[test]
    fn checksums() {
        // The well-known check values.
        assert_eq!(crc32(b"", b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"1234", b"56789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
        // The IEND chunk of every PNG file.
        let mut out = Vec::new();
        chunk(&mut out, b"IEND", &[]);
        assert_eq!(out, [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]);
    }

    #[test]
    fn stored_blocks() {
        let z = zlib_stored(&[]);
        assert_eq!(z, [0x78, 0x01, 1, 0, 0, 0xFF, 0xFF, 0, 0, 0, 1]);
        let z = zlib_stored(&[7, 8, 9]);
        assert_eq!(z[..7], [0x78, 0x01, 1, 3, 0, 0xFC, 0xFF]);
        assert_eq!(z[7..10], [7, 8, 9]);
        // Two blocks: only the last has the final bit.
        let z = zlib_stored(&vec![0u8; 65536]);
        assert_eq!(z[2], 0);
        assert_eq!(z[2 + 5 + 65535], 1);
        assert_eq!(z.len(), 2 + 5 + 65535 + 5 + 1 + 4);
    }

    #[test]
    fn bad_sizes_are_refused() {
        assert!(encode_rgb(0, 5, &[]).is_none());
        assert!(encode_rgb(5, 0, &[]).is_none());
        assert!(encode_rgb(2, 2, &[0; 15]).is_none());
        assert!(encode_rgb(2, 2, &[0; 17]).is_none());
        assert!(encode_rgb(2, 2, &[0; 16]).is_some());
    }

    #[test]
    fn the_header_says_eight_bit_rgb() {
        let file = encode_rgb(5, 3, &[0; 60]).unwrap();
        assert_eq!(file[..8], [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(file[8..16], [0, 0, 0, 13, b'I', b'H', b'D', b'R']);
        assert_eq!(file[16..24], [0, 0, 0, 5, 0, 0, 0, 3]);
        assert_eq!(file[24..29], [8, 2, 0, 0, 0]);
    }
}
