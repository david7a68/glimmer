pub mod block_allocator;
pub mod temp_allocator;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The heap does not have enough free memory to satisfy the allocation
    /// request.
    OutOfMemory,
    /// The heap is not large enough to satisfy the allocation request.
    InsufficientCapacity,
    /// The allocator does not have a access to the heap.
    NoHeap,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeapOffset(pub u64);
