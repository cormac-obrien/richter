pub struct BitSet<const N_64: usize> {
    blocks: [u64; N_64],
}

impl<const N_64: usize> BitSet<N_64> {
    pub fn new() -> Self {
        BitSet { blocks: [0; N_64] }
    }

    pub fn all_set() -> Self {
        BitSet {
            blocks: [u64::MAX; N_64],
        }
    }

    #[inline]
    fn bit_location(bit: u64) -> (u64, u64) {
        (
            bit >> 6,        // divide by 64
            1 << (bit & 63), // modulo 64
        )
    }

    #[inline]
    pub fn count(&self) -> usize {
        let mut count = 0;

        for block in self.blocks {
            count += block.count_ones() as usize;
        }

        count
    }

    #[inline]
    pub fn contains(&self, bit: u64) -> bool {
        let (index, mask) = Self::bit_location(bit);
        self.blocks[index as usize] & mask != 0
    }

    #[inline]
    pub fn set(&mut self, bit: u64) {
        let (index, mask) = Self::bit_location(bit);
        self.blocks[index as usize] |= mask;
    }

    #[inline]
    pub fn clear(&mut self, bit: u64) {
        let (index, mask) = Self::bit_location(bit);
        self.blocks[index as usize] &= !mask;
    }

    #[inline]
    pub fn toggle(&mut self, bit: u64) {
        let (index, mask) = Self::bit_location(bit);
        self.blocks[index as usize] ^= mask;
    }

    #[inline]
    pub fn iter(&self) -> BitSetIter<'_, N_64> {
        BitSetIter::new(&self.blocks)
    }
}

pub struct BitSetIter<'a, const N_64: usize> {
    block_index: usize,
    block_val: u64,
    blocks: &'a [u64; N_64],
}

impl<'a, const N_64: usize> BitSetIter<'a, N_64> {
    fn new(blocks: &'a [u64; N_64]) -> BitSetIter<'_, N_64> {
        BitSetIter {
            block_index: 0,
            block_val: blocks[0],
            blocks,
        }
    }
}

impl<'a, const N_64: usize> Iterator for BitSetIter<'a, N_64> {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        while self.block_index < N_64 {
            println!(
                "block_index = {} | block_val = {:b}",
                self.block_index, self.block_val
            );

            if self.block_val != 0 {
                // Locate the next set bit in the block.
                let next_bit = self.block_val.trailing_zeros();

                // Clear the bit.
                self.block_val &= !(1 << next_bit);

                // Return it.
                return Some((u64::BITS * self.block_index as u32 + next_bit) as u64);
            } else {
                // No set bits, move to the next block.
                self.block_index += 1;
                self.block_val = *self.blocks.get(self.block_index)?;
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_bit() {
        let mut bits: BitSet<2> = BitSet::new();

        let cases = &[0, 1, 63, 64];

        for case in cases.iter().copied() {
            bits.set(case);
            assert!(bits.contains(case));
        }
    }

    #[test]
    fn test_clear_bit() {
        let mut bits: BitSet<2> = BitSet::all_set();

        let cases = &[0, 1, 63, 64];

        for case in cases.iter().copied() {
            bits.clear(case);
            assert!(!bits.contains(case));
        }
    }

    #[test]
    fn test_iter() {
        let mut bits: BitSet<8> = BitSet::new();

        let cases = &[1, 2, 3, 5, 8, 13, 21, 34, 55, 89];

        for case in cases.iter().cloned() {
            bits.set(case);
        }

        let back = bits.iter().collect::<Vec<_>>();

        assert_eq!(&cases[..], &back);
    }
}
