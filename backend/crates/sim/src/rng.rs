#[derive(Clone, Copy, Debug)]
pub struct Pcg32 {
    pub state: u64,
    pub inc: u64,
}

impl Pcg32 {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9e3779b97f4a7c15),
            inc: 0x243f6a8885a308d3 | 1,
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6364136223846793005)
            .wrapping_add(self.inc);
        let xor = (old >> 33) ^ old;
        let mixed = xor.wrapping_mul(0xff51afd7ed558ccd);
        ((mixed >> 16) ^ mixed) as u32
    }

    pub fn next_u64(&mut self) -> u64 {
        let a = self.next_u32() as u64;
        let b = self.next_u32() as u64;
        (a << 32) | b
    }

    pub fn range(&mut self, min: u32, max: u32) -> u32 {
        if max <= min {
            return min;
        }
        min + (self.next_u32() % (max - min))
    }
}

pub const FNV_OFFSET: u64 = 0xcbf29ce484222325;
pub const FNV_PRIME: u64 = 0x100000001b3;

#[inline]
pub fn fnv_update(hash: u64, value: u64) -> u64 {
    hash.wrapping_mul(FNV_PRIME) ^ value
}

#[inline]
pub fn fnv_update_bytes(hash: u64, bytes: &[u8]) -> u64 {
    let mut h = hash;
    for b in bytes {
        h = fnv_update(h, *b as u64);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic() {
        let mut a = Pcg32::new(1234);
        let mut b = Pcg32::new(1234);
        for _ in 0..100 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }
}
