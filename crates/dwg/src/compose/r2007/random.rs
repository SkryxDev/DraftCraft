//! The random numbers of an AutoCAD 2007 file (spec §5.11): a Mersenne Twister state seeded
//! from the file header's random seed, read without tempering. Its first 128 words fill the
//! room pages leave over; the words after them hide the CRC seed in the file header and make
//! its check data.

const STATE: usize = 0x270;
const FILL: usize = 0x80;

pub(super) struct Random {
    state: [u32; STATE],
    at: usize,
    fill: [u32; FILL],
}

impl Random {
    pub(super) fn new(seed: u64) -> Random {
        let mut state = [0u32; STATE];
        let lcg = |x: u32| x.wrapping_mul(0x343FD).wrapping_add(0x26_9EC3);
        let mut v = lcg((seed >> 32) as u32);
        if let Some(s) = state.get_mut(0) {
            *s = lcg(seed as u32);
        }
        if let Some(s) = state.get_mut(1) {
            *s = v;
        }
        for (i, s) in state.iter_mut().enumerate().skip(2) {
            v = ((v >> 30) ^ v).wrapping_mul(0x6C07_8965).wrapping_add(i as u32);
            *s = v;
        }
        let mut r = Random { state, at: 0, fill: [0; FILL] };
        for i in 0..FILL {
            r.wrap();
            let w = r.word(r.at);
            if let Some(f) = r.fill.get_mut(i) {
                *f = w;
            }
            r.at += 1;
        }
        r
    }

    fn word(&self, i: usize) -> u32 {
        self.state.get(i).copied().unwrap_or(0)
    }

    /// A new state once every word has been read.
    fn wrap(&mut self) {
        if self.at < STATE {
            return;
        }
        for i in 0..STATE {
            let y = (self.word(i) & 0x8000_0000) | (self.word((i + 1) % STATE) & 0x7FFF_FFFF);
            let mut x = self.word((i + 0x18D) % STATE) ^ (y >> 1);
            if y & 1 != 0 {
                x ^= 0x9908_B0DF;
            }
            if let Some(s) = self.state.get_mut(i) {
                *s = x;
            }
        }
        self.at = 0;
    }

    /// The next word of the state.
    fn raw(&mut self) -> u32 {
        self.wrap();
        let w = self.word(self.at);
        self.at += 1;
        w
    }

    /// Words passed over.
    pub(super) fn skip(&mut self, n: usize) {
        for _ in 0..n {
            self.raw();
        }
    }

    /// The next 64-bit number: two words, the high one first.
    pub(super) fn next(&mut self) -> u64 {
        let hi = self.raw();
        u64::from(self.raw()) | (u64::from(hi) << 32)
    }

    /// `value`'s low ten bits hidden among random ones.
    pub(super) fn encode(&mut self, value: u64) -> u64 {
        let r = self.next();
        let (mut lo, mut hi) = (r as u32 & 0xDF7D_F7DF, (r >> 32) as u32 & 0xF7DF_7DF7);
        const LO: [u32; 5] = [0x20, 0x800, 0x2_0000, 0x80_0000, 0x2000_0000];
        const HI: [u32; 5] = [0x08, 0x200, 0x8000, 0x20_0000, 0x800_0000];
        for (i, bit) in LO.iter().enumerate() {
            if value & (0x200 >> i) != 0 {
                lo |= bit;
            }
        }
        for (i, bit) in HI.iter().enumerate() {
            if value & (0x10 >> i) != 0 {
                hi |= bit;
            }
        }
        u64::from(lo) | (u64::from(hi) << 32)
    }

    /// The room a page leaves over: the fill words from the first, as bytes.
    pub(super) fn fill(&self, n: usize) -> Vec<u8> {
        self.fill.iter().flat_map(|w| w.to_le_bytes()).cycle().take(n).collect()
    }
}

/// The value rotated left by its control's low five bits (spec §5.2.1.1).
pub(super) fn rotated(value: u64, control: u64) -> u64 {
    value.rotate_left((control & 0x1F) as u32)
}
