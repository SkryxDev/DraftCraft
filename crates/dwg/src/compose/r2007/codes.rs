//! The checks of an AutoCAD 2007 file (spec §5.12, §5.13): two 64-bit CRCs, a 32-bit page
//! checksum and the Reed-Solomon codes every page is stored in.
//!
//! The CRCs and the checksum take the bytes of each 8-byte block in the order 6 7 4 5 2 3 0 1
//! (16-bit words from the last), the bytes after the last whole block as 2 3 0 1 and then in
//! order when there are four or more of them, in order when fewer.

/// The bytes of `data` in the order the checks take them.
fn reordered(data: &[u8]) -> impl Iterator<Item = u8> + '_ {
    const BLOCK: [usize; 8] = [6, 7, 4, 5, 2, 3, 0, 1];
    const HALF: [usize; 4] = [2, 3, 0, 1];
    let (blocks, rest) = data.as_chunks::<8>();
    let whole = blocks.iter().flat_map(|b| BLOCK.iter().map(move |&i| b[i]));
    let (head, tail) = if rest.len() >= 4 { rest.split_at(4) } else { (&[][..], rest) };
    let head = head.iter().enumerate().map(move |(i, _)| head.get(HALF.get(i).copied().unwrap_or(i)).copied().unwrap_or(0));
    whole.chain(head).chain(tail.iter().copied())
}

/// The ECMA-182 polynomial, and the one of the mirrored CRC (reflected).
const NORMAL_POLY: u64 = 0x42F0_E1EB_A9EA_3693;
const MIRRORED_POLY: u64 = 0x95AC_9329_AC4B_C9B5;

const fn normal_table() -> [u64; 256] {
    let mut t = [0u64; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = (i as u64) << 56;
        let mut k = 0;
        while k < 8 {
            c = if c & (1 << 63) != 0 { (c << 1) ^ NORMAL_POLY } else { c << 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

const fn mirrored_table() -> [u64; 256] {
    let mut t = [0u64; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u64;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ MIRRORED_POLY } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

static NORMAL: [u64; 256] = normal_table();
static MIRRORED: [u64; 256] = mirrored_table();

fn run_normal(init: u64, data: &[u8]) -> u64 {
    reordered(data).fold(init, |c, b| NORMAL[usize::from(b ^ (c >> 56) as u8)] ^ (c << 8))
}

fn run_mirrored(init: u64, data: &[u8]) -> u64 {
    reordered(data).fold(init, |c, b| MIRRORED[usize::from(b ^ c as u8)] ^ (c >> 8))
}

/// The starting value of a check of `len` bytes from `seed`: the first form (mirrored CRCs).
fn seed1(seed: u64, len: usize) -> u64 {
    let s = seed.wrapping_add(len as u64).wrapping_mul(0x343FD).wrapping_add(0x26_9EC3);
    !(s | s.wrapping_mul(0x343FD << 32).wrapping_add(0x26_9EC3 << 32))
}

/// The second form (normal CRCs).
fn seed2(seed: u64, len: usize) -> u64 {
    let len = len as u64;
    let s = seed.wrapping_add(len).wrapping_mul(0x343FD).wrapping_add(0x26_9EC3);
    !s.wrapping_mul(0x1_0003_43FD).wrapping_add(len.wrapping_add(0x26_9EC3))
}

/// The normal CRC: of the file header's fields and of compressed data.
pub(super) fn normal(seed: u64, data: &[u8]) -> u64 {
    !run_normal(seed2(seed, data.len()), data)
}

/// The normal CRC started the first way: the file header's checking sequence.
pub(super) fn normal_seed1(seed: u64, data: &[u8]) -> u64 {
    !run_normal(seed1(seed, data.len()), data)
}

/// The mirrored CRC: of pages.
pub(super) fn mirrored(seed: u64, data: &[u8]) -> u64 {
    run_mirrored(seed1(seed, data.len()), data)
}

/// The two CRCs of the file header's check data, started from a random number.
pub(super) fn check_normal(random: u64, data: &[u8]) -> u64 {
    !run_normal(!random, data)
}

pub(super) fn check_mirrored(random: u64, data: &[u8]) -> u64 {
    run_mirrored(!random, data)
}

/// The checksum of a data page's bytes (an Adler-32 started from the length).
pub(super) fn page_checksum(data: &[u8]) -> u32 {
    let s = (data.len() as u64).wrapping_mul(0x343FD).wrapping_add(0x26_9EC3);
    let (mut a, mut b) = ((s & 0xFFFF) as u32, ((s >> 16) & 0xFFFF) as u32);
    for chunk in data.chunks(0x15B0) {
        for x in reordered(chunk) {
            a += u32::from(x);
            b += a;
        }
        a %= 0xFFF1;
        b %= 0xFFF1;
    }
    (b << 16) | a
}

/// A codeword: 255 bytes, of which 251 data in a data page and 239 in the file header and the
/// maps.
pub(super) const CODEWORD: usize = 255;
pub(super) const PAGE_DATA: usize = 251;
pub(super) const SYSTEM_DATA: usize = 239;

/// GF(256) for one primitive polynomial: powers of the generator and their logarithms.
struct Field {
    exp: [u8; 512],
    log: [u8; 256],
}

const fn field(poly: u16) -> Field {
    let mut exp = [0u8; 512];
    let mut log = [0u8; 256];
    let mut x: u16 = 1;
    let mut i = 0;
    while i < 255 {
        exp[i] = x as u8;
        exp[i + 255] = x as u8;
        log[x as usize] = i as u8;
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= poly;
        }
        i += 1;
    }
    Field { exp, log }
}

impl Field {
    fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            return 0;
        }
        let i = usize::from(self.log[usize::from(a)]) + usize::from(self.log[usize::from(b)]);
        self.exp.get(i).copied().unwrap_or(0)
    }
}

/// The code of the data pages and the one of the file header and the maps: the field's
/// polynomial, and the generator's roots from α^k for k data bytes.
static DATA_FIELD: Field = field(0x11D);
static SYSTEM_FIELD: Field = field(0x169);

/// The generator polynomial of `parity` check bytes, highest power first (its leading 1 left
/// out), with roots α^first … α^(first + parity − 1).
fn generator(f: &Field, parity: usize, first: usize) -> Vec<u8> {
    let mut g = vec![1u8];
    for i in 0..parity {
        let root = f.exp.get((first + i) % 255).copied().unwrap_or(1);
        let mut next = vec![0u8; g.len() + 1];
        for (j, &c) in g.iter().enumerate() {
            if let Some(n) = next.get_mut(j) {
                *n ^= c;
            }
            if let Some(n) = next.get_mut(j + 1) {
                *n ^= f.mul(c, root);
            }
        }
        g = next;
    }
    g.into_iter().skip(1).collect()
}

/// The check bytes of one block of `data.len()` data bytes (the first is the highest power).
fn parity(f: &Field, poly: &[u8], data: &[u8]) -> Vec<u8> {
    let mut reg = vec![0u8; poly.len()];
    for &d in data {
        let fb = d ^ reg.first().copied().unwrap_or(0);
        reg.rotate_left(1);
        if let Some(last) = reg.last_mut() {
            *last = 0;
        }
        for (r, &g) in reg.iter_mut().zip(poly) {
            *r ^= f.mul(fb, g);
        }
    }
    reg
}

/// A Reed-Solomon code: data bytes per codeword and its field.
#[derive(Clone, Copy)]
pub(super) enum Code {
    /// Data pages: 251 data bytes, 4 check bytes.
    Page,
    /// The file header and the maps: 239 data bytes, 16 check bytes.
    System,
}

impl Code {
    pub(super) fn data(self) -> usize {
        match self {
            Code::Page => PAGE_DATA,
            Code::System => SYSTEM_DATA,
        }
    }
    fn field(self) -> &'static Field {
        match self {
            Code::Page => &DATA_FIELD,
            Code::System => &SYSTEM_FIELD,
        }
    }

    /// The check bytes of each block of `data` (a whole number of blocks), block after block.
    pub(super) fn parities(self, data: &[u8]) -> Vec<Vec<u8>> {
        let k = self.data();
        let poly = generator(self.field(), CODEWORD - k, k);
        data.chunks(k).map(|block| parity(self.field(), &poly, block)).collect()
    }

    /// `data` (a whole number of blocks) as interleaved codewords: byte `j` of codeword `i` at
    /// `i + j * blocks`.
    pub(super) fn interleaved(self, data: &[u8]) -> Vec<u8> {
        let k = self.data();
        let blocks = data.len().div_ceil(k);
        let mut out = vec![0u8; blocks * CODEWORD];
        for (i, (block, check)) in data.chunks(k).zip(self.parities(data)).enumerate() {
            for (j, &b) in block.iter().chain(&check).enumerate() {
                if let Some(o) = out.get_mut(i + j * blocks) {
                    *o = b;
                }
            }
        }
        out
    }
}
