//! Pure-Rust FIPS 180-4 SHA-256 implementation for content-addressed evidence integrity.
//!
//! Content hashes establish byte integrity after acquisition; they do not establish
//! source truth, authenticity, or merge distinct publisher releases.

use std::fmt;

/// A validated 32-byte (64-hex-character) SHA-256 content digest.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Computes the FIPS 180-4 SHA-256 digest of `bytes`.
    #[must_use]
    pub fn compute(bytes: &[u8]) -> Self {
        Self(sha256_digest(bytes))
    }

    /// Parses a 64-character lowercase or uppercase ASCII hex string into a [`ContentHash`].
    pub fn from_hex(hex: &str) -> Result<Self, HashParseError> {
        if hex.len() != 64 {
            return Err(HashParseError::InvalidLength(hex.len()));
        }
        let mut out = [0u8; 32];
        for (idx, chunk) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let hi = decode_nibble(chunk[0]).ok_or(HashParseError::InvalidHexByte(idx * 2))?;
            let lo = decode_nibble(chunk[1]).ok_or(HashParseError::InvalidHexByte(idx * 2 + 1))?;
            if let Some(slot) = out.get_mut(idx) {
                *slot = (hi << 4) | lo;
            }
        }
        Ok(Self(out))
    }

    /// Returns the raw 32-byte digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Formats the digest as a 64-character lowercase hexadecimal string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(64);
        for &b in &self.0 {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0x0f) as usize] as char);
        }
        s
    }

    /// Verifies that `bytes` match this [`ContentHash`] using a bitwise constant-length check.
    #[must_use]
    pub fn verify_bytes(&self, bytes: &[u8]) -> bool {
        let actual = Self::compute(bytes);
        let mut diff = 0u8;
        for (a, b) in self.0.iter().zip(actual.0.iter()) {
            diff |= a ^ b;
        }
        diff == 0
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", self.to_hex())
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Errors returned when parsing a hex-encoded [`ContentHash`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HashParseError {
    /// Input string length was not 64 ASCII characters.
    InvalidLength(usize),
    /// Byte at the given 0-based index was not a valid hex character.
    InvalidHexByte(usize),
}

impl fmt::Display for HashParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(len) => {
                write!(f, "SHA-256 hex digest must be 64 characters, got {len}")
            }
            Self::InvalidHexByte(idx) => {
                write!(f, "invalid hexadecimal character at byte offset {idx}")
            }
        }
    }
}

impl std::error::Error for HashParseError {}

const fn decode_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn compress_block(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (i, chunk) in block.as_chunks::<4>().0.iter().enumerate() {
        w[i] = u32::from_be_bytes(*chunk);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }

    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    let mut f = state[5];
    let mut g = state[6];
    let mut h = state[7];

    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

fn sha256_digest(bytes: &[u8]) -> [u8; 32] {
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let (blocks, rem) = bytes.as_chunks::<64>();
    for block in blocks {
        compress_block(&mut state, block);
    }
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let mut final_block = [0u8; 64];
    final_block[..rem.len()].copy_from_slice(rem);
    final_block[rem.len()] = 0x80;

    if rem.len() >= 56 {
        compress_block(&mut state, &final_block);
        final_block = [0u8; 64];
    }
    final_block[56..64].copy_from_slice(&bit_len.to_be_bytes());
    compress_block(&mut state, &final_block);

    let mut out = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        let be = word.to_be_bytes();
        out[i * 4..i * 4 + 4].copy_from_slice(&be);
    }
    out
}
