//! LZMA-style adaptive binary range coder with 11-bit probabilities.
//!
//! Carries ripple back into bytes already written; `finish` adds a 2-byte flush. Reading past
//! the end of the data yields zeros, so a truncated stream decodes without panicking.

const PROB_BITS: u32 = 11;
const ADAPT: u32 = 4; // adaptation rate: p moves 1/16 of the way per decision

pub(crate) struct RcEnc {
    low: u64,
    range: u32,
    pub(crate) out: Vec<u8>,
}

impl RcEnc {
    pub(crate) fn new() -> Self {
        RcEnc { low: 0, range: u32::MAX, out: vec![] }
    }
    fn carry(&mut self) {
        if self.low >= 1 << 32 {
            self.low -= 1 << 32;
            let mut i = self.out.len();
            while i > 0 {
                i -= 1;
                if self.out[i] == 0xFF {
                    self.out[i] = 0;
                } else {
                    self.out[i] += 1;
                    break;
                }
            }
        }
    }
    pub(crate) fn bit(&mut self, p: &mut u16, b: bool) {
        let bound = (self.range >> PROB_BITS) * *p as u32;
        if b {
            self.low += bound as u64;
            self.range -= bound;
            *p -= *p >> ADAPT;
            self.carry();
        } else {
            self.range = bound;
            *p += ((1 << PROB_BITS) - *p) >> ADAPT;
        }
        if self.range < 1 << 24 {
            self.range <<= 8;
            self.out.push((self.low >> 24) as u8);
            self.low = (self.low << 8) & 0xFFFF_FFFF;
        }
    }
    /// Flush: the top 2 bytes of a value in [low, low+range) whose low 16 bits are zero.
    pub(crate) fn finish(mut self) -> Vec<u8> {
        self.low = (self.low + 0xFFFF) & !0xFFFF;
        self.carry();
        self.out.push((self.low >> 24) as u8);
        self.out.push((self.low >> 16) as u8);
        self.out
    }
}

pub(crate) struct RcDec<'a> {
    data: &'a [u8],
    pos: usize,
    code: u32,
    range: u32,
}

impl<'a> RcDec<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        let mut d = RcDec { data, pos: 0, code: 0, range: u32::MAX };
        for _ in 0..4 {
            d.code = (d.code << 8) | d.next() as u32;
        }
        d
    }
    fn next(&mut self) -> u8 {
        let b = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        b
    }
    pub(crate) fn bit(&mut self, p: &mut u16) -> bool {
        let bound = (self.range >> PROB_BITS) * *p as u32;
        let b = if self.code < bound {
            self.range = bound;
            *p += ((1 << PROB_BITS) - *p) >> ADAPT;
            false
        } else {
            self.code -= bound;
            self.range -= bound;
            *p -= *p >> ADAPT;
            true
        };
        if self.range < 1 << 24 {
            self.range <<= 8;
            self.code = (self.code << 8) | self.next() as u32;
        }
        b
    }
}
