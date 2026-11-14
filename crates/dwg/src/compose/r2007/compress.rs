//! The compression of AutoCAD 2007 pages (spec §5.10): the mirror of
//! [`crate::read::r2007::decompress`].
//!
//! A stream is a run of at least 8 literal bytes, then copies of earlier output. Each copy says
//! how many literal bytes (none to 7) follow it; a longer run follows a copy that says none, with
//! a length opcode of its own. Literal bytes are stored in the order the format shuffles them in.
//! Copies are found greedily, through chains of the positions where each three bytes were seen.

use crate::read::r2007::LITERAL_ORDER;

/// The shortest copy worth its instruction.
const MIN_COPY: usize = 3;
/// How far back a copy reaches, and how long it can be (the 16-bit forms).
const WINDOW: usize = 0xFFFF;
const MAX_COPY: usize = 0x100 + 0xFFFF;
/// How many earlier positions of the same three bytes are tried.
const TRIES: usize = 48;
const HASH_BITS: u32 = 15;

/// `src` compressed; `None` when it is too short for a stream (fewer than 8 bytes), which is
/// then stored as it is.
pub(super) fn compress(src: &[u8]) -> Option<Vec<u8>> {
    if src.len() < 8 {
        return None;
    }
    let mut out = Vec::with_capacity(src.len() / 2 + 16);
    let mut finder = Finder::new(src);
    // The first run: at least 8 bytes, and up to the first copy after them.
    let mut literal_from = 0usize;
    let mut pos = 0usize;
    while pos < 8 {
        finder.insert(pos);
        pos += 1;
    }
    let mut first = true;
    // The opcode byte of the last copy, whose low three bits count the literal bytes after it.
    let mut last_copy: Option<usize> = None;
    while pos < src.len() {
        let Some((len, back)) = finder.longest(pos) else {
            finder.insert(pos);
            pos += 1;
            continue;
        };
        literals(&mut out, src.get(literal_from..pos).unwrap_or(&[]), first, last_copy);
        first = false;
        last_copy = Some(copy(&mut out, len, back));
        for p in pos..pos + len {
            finder.insert(p);
        }
        pos += len;
        literal_from = pos;
    }
    literals(&mut out, src.get(literal_from..).unwrap_or(&[]), first, last_copy);
    Some(out)
}

/// A run of literal bytes: after the copy whose last byte is at `last_copy`, in its low bits when
/// the run is short; with a length opcode of its own when it is long or starts the stream.
fn literals(out: &mut Vec<u8>, run: &[u8], first: bool, last_copy: Option<usize>) {
    if run.is_empty() {
        return;
    }
    if !first
        && run.len() < 8
        && let Some(b) = last_copy.and_then(|at| out.get_mut(at))
    {
        *b |= run.len() as u8;
    } else {
        let mut rest = run.len() - 8;
        if rest < 0x0F {
            out.push(rest as u8);
        } else {
            rest -= 0x0F;
            out.push(0x0F);
            if rest < 0xFF {
                out.push(rest as u8);
            } else {
                out.push(0xFF);
                rest -= 0xFF;
                while rest >= 0xFFFF {
                    out.extend_from_slice(&[0xFF, 0xFF]);
                    rest -= 0xFFFF;
                }
                out.extend_from_slice(&(rest as u16).to_le_bytes());
            }
        }
    }
    for piece in run.chunks(32) {
        shuffled(out, piece);
    }
}

/// A piece of at most 32 literal bytes, in the order the reader puts back together.
fn shuffled(out: &mut Vec<u8>, piece: &[u8]) {
    let mut stored = vec![0u8; piece.len()];
    let mut next = piece.iter();
    for &(start, count) in LITERAL_ORDER.get(piece.len()).copied().unwrap_or(&[]) {
        let (start, count) = (usize::from(start), usize::from(count));
        let backwards = count == 2 || count == 3;
        for i in 0..count {
            let at = if backwards { start + count - 1 - i } else { start + i };
            if let (Some(s), Some(&b)) = (stored.get_mut(at), next.next()) {
                *s = b;
            }
        }
    }
    out.extend_from_slice(&stored);
}

/// A copy of `len` bytes from `back` bytes back; the index of its last byte, whose low three bits
/// are left for the count of literal bytes after it.
fn copy(out: &mut Vec<u8>, len: usize, back: usize) -> usize {
    let b = back - 1;
    if (MIN_COPY..=14).contains(&len) && b < 0x200 {
        out.push(((len as u8) << 4) | (b & 0x0F) as u8);
        out.push(((b >> 4) << 3) as u8);
    } else if (MIN_COPY..=18).contains(&len) && b < 0x2000 {
        out.push(0x10 | (len - 3) as u8);
        out.push(b as u8);
        out.push(((b >> 8) << 3) as u8);
    } else if len < 0x100 {
        out.push(0x20 | (len & 7) as u8);
        out.extend_from_slice(&(back as u16).to_le_bytes());
        out.push((len & 0xF8) as u8);
    } else {
        let l = len - 0x100;
        out.push(0x28 | (l & 7) as u8);
        out.extend_from_slice(&(b as u16).to_le_bytes());
        out.push((l >> 3) as u8);
        out.push(((l >> 11) << 3) as u8);
    }
    out.len() - 1
}

/// Where each three bytes were seen: the last position of each hash, and before each position
/// the previous one of its hash.
struct Finder<'a> {
    src: &'a [u8],
    head: Vec<usize>,
    prev: Vec<usize>,
}

const NONE: usize = usize::MAX;

impl<'a> Finder<'a> {
    fn new(src: &'a [u8]) -> Finder<'a> {
        Finder { src, head: vec![NONE; 1 << HASH_BITS], prev: vec![NONE; src.len()] }
    }

    fn hash(&self, pos: usize) -> Option<usize> {
        let b = self.src.get(pos..pos + MIN_COPY)?;
        let v = u32::from(b[0]) | (u32::from(b[1]) << 8) | (u32::from(b[2]) << 16);
        Some((v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize)
    }

    fn insert(&mut self, pos: usize) {
        if let Some(h) = self.hash(pos)
            && let (Some(head), Some(prev)) = (self.head.get(h).copied(), self.prev.get_mut(pos))
        {
            *prev = head;
            if let Some(slot) = self.head.get_mut(h) {
                *slot = pos;
            }
        }
    }

    /// The longest copy for the bytes at `pos`: its length and how far back it starts.
    fn longest(&self, pos: usize) -> Option<(usize, usize)> {
        let h = self.hash(pos)?;
        let limit = (self.src.len() - pos).min(MAX_COPY);
        let here = self.src.get(pos..pos + limit)?;
        let mut best: Option<(usize, usize)> = None;
        let mut at = self.head.get(h).copied().unwrap_or(NONE);
        for _ in 0..TRIES {
            if at == NONE || pos - at > WINDOW {
                break;
            }
            let there = self.src.get(at..).unwrap_or(&[]);
            let len = here.iter().zip(there).take_while(|(a, b)| a == b).count();
            if len >= MIN_COPY && best.is_none_or(|(l, _)| len > l) {
                best = Some((len, pos - at));
                if len == limit {
                    break;
                }
            }
            at = self.prev.get(at).copied().unwrap_or(NONE);
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::compress;
    use crate::read::r2007::decompress;

    fn round_trip(data: &[u8]) {
        match compress(data) {
            None => assert!(data.len() < 8),
            Some(c) => assert_eq!(decompress(&c, data.len()).unwrap(), data, "{} bytes", data.len()),
        }
    }

    #[test]
    fn streams_decompress_to_their_data() {
        for n in 0..300 {
            round_trip(&(0..n).map(|i| (i * 7 % 13) as u8).collect::<Vec<_>>());
            round_trip(&vec![0u8; n]);
        }
        // Pseudo-random data, with and without repeats; long runs and far copies.
        let mut x = 0x1234_5678u32;
        let mut noise = Vec::new();
        for _ in 0..70_000 {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            noise.push(x as u8);
        }
        round_trip(&noise);
        let mut mixed = noise[..3000].to_vec();
        mixed.extend_from_slice(&[9; 70_000]);
        mixed.extend_from_slice(&noise[..3000]);
        mixed.extend_from_slice(&noise[100..400]);
        round_trip(&mixed);
        for n in [8, 9, 15, 16, 22, 23, 24, 31, 32, 33, 0x115, 0x116, 0x117, 0x10114, 0x10115, 0x10116] {
            round_trip(&noise[..n]);
            let mut lit = noise[..n].to_vec();
            lit.extend_from_slice(&[1, 2, 3, 1, 2, 3, 1, 2, 3]);
            lit.extend_from_slice(&noise[n..n + 5]);
            round_trip(&lit);
        }
    }

    #[test]
    fn repeats_shrink() {
        let data: Vec<u8> = (0..0xF480).map(|i| (i % 97) as u8).collect();
        assert!(compress(&data).unwrap().len() < data.len() / 20);
    }
}
