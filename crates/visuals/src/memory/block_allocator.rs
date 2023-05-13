use super::HeapOffset;

use super::Error;

pub struct BlockAllocator {
    blocks: Box<[Block]>,
    block_size: HeapOffset,
    first_free_block: usize,
}

impl BlockAllocator {
    pub fn new(block_size: HeapOffset, max_blocks: u32) -> Self {
        assert!(block_size.0 > 0);
        assert!((block_size.0 * max_blocks as u64) < u64::MAX);

        // This might overflow the stack, but it's fine for now. If this becomes
        // a problem, just turn this into a Box<[Block]>.
        let mut blocks = vec![Block::default(); max_blocks as usize].into_boxed_slice();
        for (i, block) in blocks.iter_mut().enumerate() {
            block.start = HeapOffset(i as u64 * block_size.0);
            block.next = i + 1;
        }

        Self {
            blocks,
            block_size,
            first_free_block: 0,
        }
    }

    pub fn allocate(&mut self) -> Result<HeapOffset, Error> {
        if self.first_free_block < self.blocks.len() {
            let block = &mut self.blocks[self.first_free_block];
            self.first_free_block = block.next;
            Ok(block.start)
        } else {
            Err(Error::OutOfMemory {
                capacity: self.blocks.len() as u64,
                available: 0,
                requested: 1,
            })
        }
    }

    pub fn free(&mut self, offset: HeapOffset) {
        assert_eq!(offset.0 % self.block_size.0, 0);

        let block_index = offset.0 as usize / self.block_size.0 as usize;
        assert!(block_index < self.blocks.len());

        let block = &mut self.blocks[block_index];
        block.next = self.first_free_block;
        self.first_free_block = block_index;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Block {
    start: HeapOffset,
    next: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let mut allocator = BlockAllocator::new(HeapOffset(8), 4);

        let a0 = allocator.allocate().unwrap();
        let a1 = allocator.allocate().unwrap();
        let a2 = allocator.allocate().unwrap();
        let a3 = allocator.allocate().unwrap();

        assert_eq!(a0, HeapOffset(0));
        assert_eq!(a1, HeapOffset(8));
        assert_eq!(a2, HeapOffset(16));
        assert_eq!(a3, HeapOffset(24));

        assert_eq!(
            allocator.allocate(),
            Err(Error::OutOfMemory {
                capacity: 4,
                available: 0,
                requested: 1,
            })
        );

        allocator.free(a1);
        allocator.free(a2);

        let a1 = allocator.allocate().unwrap();
        let a2 = allocator.allocate().unwrap();

        assert_eq!(a1, HeapOffset(16));
        assert_eq!(a2, HeapOffset(8));

        assert_eq!(
            allocator.allocate(),
            Err(Error::OutOfMemory {
                capacity: 4,
                available: 0,
                requested: 1,
            })
        );
    }
}
