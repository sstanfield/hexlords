//! A PNG writer: 8-bit RGBA, one IDAT chunk, the "Sub" filter on every row,
//! and a small deflate (LZ77 with a hash chain, the fixed Huffman codes). It
//! is not the smallest encoding, but tiles of smooth color shrink to a
//! fraction of their size, and any PNG reader reads it.

/// The CRC-32 table of PNG's chunks.
fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for (n, entry) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *entry = c;
    }
    table
}

/// Append a chunk: its length, its type, its data and the CRC of the last two.
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8], table: &[u32; 256]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut c = 0xffff_ffffu32;
    for &b in &out[start..] {
        c = table[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
    }
    out.extend_from_slice(&(c ^ 0xffff_ffff).to_be_bytes());
}

/// A stream of bits, the lowest bit of a byte first, as deflate packs them.
struct Bits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Bits {
    /// `len` bits of `v`, lowest first (how deflate stores plain numbers).
    fn put(&mut self, v: u32, len: u32) {
        self.acc |= v << self.n;
        self.n += len;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// A Huffman code of `len` bits, highest bit first (how deflate stores codes).
    fn put_code(&mut self, code: u32, len: u32) {
        let mut reversed = 0;
        for i in 0..len {
            reversed |= ((code >> i) & 1) << (len - 1 - i);
        }
        self.put(reversed, len);
    }

    /// A literal byte or a length symbol, in the fixed Huffman code.
    fn symbol(&mut self, v: u32) {
        match v {
            0..=143 => self.put_code(0x30 + v, 8),
            144..=255 => self.put_code(0x190 + v - 144, 9),
            256..=279 => self.put_code(v - 256, 7),
            _ => self.put_code(0xc0 + v - 280, 8),
        }
    }
}

/// Deflate's tables: where each length and distance symbol starts, and how
/// many extra bits follow it.
const LENGTH_BASE: [u32; 29] =
    [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LENGTH_EXTRA: [u32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DISTANCE_BASE: [u32; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577,
];
const DISTANCE_EXTRA: [u32; 30] =
    [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// The longest match deflate can say, and how far back one may lie.
const MAX_MATCH: usize = 258;
const WINDOW: usize = 32768;
/// How many earlier places with the same three bytes are tried.
const MAX_TRIES: u32 = 32;
const HASH_BITS: usize = 15;
/// No earlier place.
const NONE: usize = usize::MAX;

/// `data` as a zlib stream: one deflate block in the fixed Huffman codes.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut bits = Bits { out: vec![0x78, 0x01], acc: 0, n: 0 };
    bits.put(1, 1); // the last block
    bits.put(1, 2); // fixed Huffman codes
    // The last place each hash of three bytes was seen, and for each place the
    // one before it with the same hash.
    let mut head = vec![NONE; 1 << HASH_BITS];
    let mut prev = vec![NONE; data.len()];
    let hash = |i: usize| -> usize {
        ((usize::from(data[i]) << 10) ^ (usize::from(data[i + 1]) << 5) ^ usize::from(data[i + 2]))
            & ((1 << HASH_BITS) - 1)
    };
    let mut i = 0;
    while i < data.len() {
        let (mut best_len, mut best_distance) = (0, 0);
        if i + 2 < data.len() {
            let h = hash(i);
            let mut j = head[h];
            let mut tries = MAX_TRIES;
            while j != NONE && i - j <= WINDOW && tries > 0 {
                let mut len = 0;
                while len < MAX_MATCH && i + len < data.len() && data[j + len] == data[i + len] {
                    len += 1;
                }
                if len > best_len {
                    (best_len, best_distance) = (len, i - j);
                }
                j = prev[j];
                tries -= 1;
            }
            prev[i] = head[h];
            head[h] = i;
        }
        if best_len >= 3 {
            let len = best_len as u32;
            let symbol = LENGTH_BASE.iter().rposition(|&base| base <= len).unwrap_or(0);
            bits.symbol(257 + symbol as u32);
            bits.put(len - LENGTH_BASE[symbol], LENGTH_EXTRA[symbol]);
            let distance = best_distance as u32;
            let symbol = DISTANCE_BASE.iter().rposition(|&base| base <= distance).unwrap_or(0);
            bits.put_code(symbol as u32, 5);
            bits.put(distance - DISTANCE_BASE[symbol], DISTANCE_EXTRA[symbol]);
            // The places the match covers can start later matches.
            for (k, before) in prev.iter_mut().enumerate().take(i + best_len).skip(i + 1) {
                if k + 2 < data.len() {
                    let h = hash(k);
                    *before = head[h];
                    head[h] = k;
                }
            }
            i += best_len;
        } else {
            bits.symbol(u32::from(data[i]));
            i += 1;
        }
    }
    bits.symbol(256); // the end of the block
    if bits.n > 0 {
        bits.out.push(bits.acc as u8);
    }
    // zlib's Adler-32 of the data.
    let (mut s1, mut s2) = (1u32, 0u32);
    for &b in data {
        s1 = (s1 + u32::from(b)) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    bits.out.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());
    bits.out
}

/// A `w` x `h` image of straight-alpha RGBA, top row first, as a PNG file.
/// `None` if a side is zero or `rgba` is not `w * h * 4` bytes.
pub fn encode_rgba(w: usize, h: usize, rgba: &[u8]) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || w > u32::MAX as usize || h > u32::MAX as usize || rgba.len() != w * h * 4 {
        return None;
    }
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for row in rgba.chunks_exact(w * 4) {
        // The "Sub" filter: each byte less the one a pixel to its left, which
        // makes smooth gradients repeat.
        raw.push(1);
        for (x, &byte) in row.iter().enumerate() {
            let left = if x >= 4 { row[x - 4] } else { 0 };
            raw.push(byte.wrapping_sub(left));
        }
    }
    let table = crc_table();
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut header = Vec::new();
    header.extend_from_slice(&(w as u32).to_be_bytes());
    header.extend_from_slice(&(h as u32).to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits a channel, RGBA, not interlaced
    chunk(&mut out, b"IHDR", &header, &table);
    chunk(&mut out, b"IDAT", &deflate(&raw), &table);
    chunk(&mut out, b"IEND", &[], &table);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use turbo_ferret::image::{Limits, png};

    /// Decode with the decoder the game loads its tiles with.
    fn decode(bytes: &[u8]) -> (i32, i32, Vec<u8>) {
        let image = png::decode(bytes, &Limits::DEFAULT).expect("a PNG the game can read");
        (image.size.w, image.size.h, image.rgba)
    }

    #[test]
    fn a_file_has_the_signature_three_chunks_and_pngs_iend() {
        let bytes = encode_rgba(1, 1, &[1, 2, 3, 4]).unwrap();
        assert_eq!(bytes[..8], [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(bytes[8..16], [0, 0, 0, 13, b'I', b'H', b'D', b'R']);
        assert_eq!(bytes[16..29], [0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
        // Every PNG ends in the same twelve bytes: the CRC of "IEND" is known.
        assert_eq!(bytes[bytes.len() - 12..], [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]);
        assert_eq!(decode(&bytes), (1, 1, vec![1, 2, 3, 4]));
    }

    #[test]
    fn pictures_come_back_as_they_went_in() {
        // Flat color (long matches), a gradient (the filter's case), noise (no
        // matches: literals above 143 take nine bits), and all three in one.
        let (w, h) = (97, 61);
        let mut flat = Vec::new();
        let mut gradient = Vec::new();
        let mut noise = Vec::new();
        let mut state = 12345u32;
        for y in 0..h {
            for x in 0..w {
                flat.extend_from_slice(&[40, 90, 200, 255]);
                gradient.extend_from_slice(&[(x * 2) as u8, (y * 4) as u8, ((x + y) % 256) as u8, 255]);
                for _ in 0..4 {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    noise.push((state >> 24) as u8);
                }
            }
        }
        let mixed: Vec<u8> = flat.iter().chain(&gradient).chain(&noise).copied().collect();
        for (name, pixels, height) in
            [("flat", &flat, h), ("gradient", &gradient, h), ("noise", &noise, h), ("mixed", &mixed, 3 * h)]
        {
            let bytes = encode_rgba(w, height, pixels).unwrap();
            assert_eq!(decode(&bytes), (w as i32, height as i32, pixels.clone()), "{name}");
        }
        // And the flat one is far smaller than its pixels.
        assert!(encode_rgba(w, h, &flat).unwrap().len() < flat.len() / 20);
    }

    #[test]
    fn a_match_can_be_as_long_and_as_far_back_as_deflate_allows() {
        // 40000 bytes that only repeat from 32768 back, in an image one pixel high.
        let mut state = 7u32;
        let block: Vec<u8> = (0..WINDOW)
            .map(|_| {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
                (state >> 16) as u8
            })
            .collect();
        let pixels: Vec<u8> = block.iter().chain(&block[..7232]).copied().collect();
        let bytes = encode_rgba(pixels.len() / 4, 1, &pixels).unwrap();
        assert_eq!(decode(&bytes).2, pixels);
    }

    #[test]
    fn a_picture_that_is_no_picture_is_none() {
        assert!(encode_rgba(0, 4, &[]).is_none());
        assert!(encode_rgba(4, 0, &[]).is_none());
        assert!(encode_rgba(2, 2, &[0; 15]).is_none());
    }

    #[test]
    fn deflate_ends_in_the_datas_adler_checksum() {
        // Adler-32 of "Wikipedia" is 0x11E60398.
        let stream = deflate(b"Wikipedia");
        assert_eq!(stream[..2], [0x78, 0x01]);
        assert_eq!(stream[stream.len() - 4..], [0x11, 0xE6, 0x03, 0x98]);
    }
}
