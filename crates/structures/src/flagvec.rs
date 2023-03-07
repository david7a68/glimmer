/// A vector of single-bit flags.
///
/// This is a reimplementation instead of using a `Vec<bool>` because the latter
/// wastes 7 bits per flag, and instead of using the `bitvec` crate because we
/// don't need pretty much any of the features that it provides.
///
/// The size of the vector grows dynamically as indices are set, but never
/// shrinks.
pub struct FlagVec {
    data: Vec<u64>,
    length: usize,
}

impl Default for FlagVec {
    fn default() -> Self {
        Self::new()
    }
}

impl FlagVec {
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            length: 0,
        }
    }

    /// Returns the index of the highest set bit + 1. This value is 'sticky' and
    /// will never decrease, even if that bit is later cleared.
    #[must_use]
    pub fn len(&self) -> usize {
        self.length
    }

    /// Returns true if no flags are set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Sets the flag at the given index to the given value, and returns the old
    /// value.
    ///
    /// The vector will grow as needed to accommodate the given index.
    pub fn set(&mut self, index: usize, value: bool) -> bool {
        let int_index = index / 64;
        let bit_index = index % 64;

        if int_index >= self.data.len() {
            self.data.resize(int_index + 1, 0);

            // basically, max(self.length, index + 1)
            self.length = index + 1;
        }

        let old_value = self.data[int_index] & (1 << bit_index) != 0;

        self.data[int_index] =
            (self.data[int_index] & !(1 << bit_index)) | u64::from(value) << bit_index;

        old_value
    }

    /// Returns the value of the flag at the given index, or false if the index
    /// is out of bounds.
    #[must_use]
    pub fn get(&self, index: usize) -> bool {
        let int_index = index / 64;
        let bit_index = index % 64;

        if int_index >= self.data.len() {
            false
        } else {
            self.data[int_index] & (1 << bit_index) != 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanity() {
        let mut vec = FlagVec::new();

        assert_eq!(vec.len(), 0);
        assert_eq!(vec.get(0), false);
        assert_eq!(vec.get(1_000_000_000), false);

        vec.set(0, true);
        assert_eq!(vec.len(), 1);

        vec.set(1_000_000_000, true);
        assert_eq!(vec.len(), 1_000_000_001);
        assert_eq!(vec.data.len(), 1_000_000_001 / 64 + 1);

        vec.set(1_000_000_000, false);
        assert_eq!(vec.get(1_000_000_000), false);
    }
}
