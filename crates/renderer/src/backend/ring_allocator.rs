use std::ffi::c_void;

use crate::backend::next_multiple_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The allocator does not have enough free memory to satisfy the
    /// allocation.
    OutOfMemory,
    /// The allocator is not large enough to satisfy the allocation.
    InsufficientCapacity,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Allocation {
    /// The size of the allocation.
    size: u32,

    /// The allocation number.
    ///
    /// This is used to enforce the ordering of allocations and deallocations.
    alloc_id: u32,

    /// The offset from the base of the heap to the start of the allocation.
    offset: u64,
}

/// An allocator of memory from a fixed-size ring buffer.
///
/// Memory must be freed in the same order it was allocated.
///
/// ## Example
pub struct RingAllocator {
    capacity: u64,
    heap_ptr: *mut c_void,
    bytes_allocated: u64,
    bytes_deallocated: u64,
    allocations_served: u32,
    allocations_freed: u32,
}

impl RingAllocator {
    pub fn new(capacity: u64, heap_ptr: *mut c_void) -> Self {
        Self {
            capacity,
            heap_ptr,
            bytes_allocated: 0,
            bytes_deallocated: 0,
            allocations_served: 0,
            allocations_freed: 0,
        }
    }

    /// Returns true if the allocator has no extant allocations.
    pub fn is_free(&self) -> bool {
        self.bytes_allocated == self.bytes_deallocated
    }

    /// Returns true if the allocator cannot satisfy any more allocations.
    pub fn is_full(&self) -> bool {
        self.bytes_allocated - self.bytes_deallocated == self.capacity
    }

    /// Allocates a region of `size` bytes aligned to `align`.
    ///
    /// Alignment is performed relative to the start of the heap pointer passed
    /// into the constructor.
    ///
    /// ## Errors
    ///
    /// Returns `Error::InsufficientCapacity` if the allocator is not large
    /// enough to satisfy the request, and `Error::OutOfMemory` if the allocator
    /// cannot satisfy the request due to extant allocations.
    ///
    /// ## Panics
    ///
    /// This function will panic if `align` is not a power of two, or if the
    /// aligned allocation size is larger than 4GB.
    pub fn allocate(&mut self, size: u64, align: u64) -> Result<(Allocation, &mut [u8]), Error> {
        let aligned_size = next_multiple_of(size, align);

        assert!(
            aligned_size <= u32::MAX as u64,
            "allocation request exceeds 4GB per-allocation limit"
        );

        if aligned_size > self.capacity {
            return Err(Error::InsufficientCapacity);
        }

        // allocate from head, free from tail
        let heap_tail = self.bytes_deallocated % self.capacity;
        let heap_head = self.bytes_allocated % self.capacity;

        // Handle potentially unaligned heap_ptr.
        let aligned_head_amount =
            unsafe { self.heap_ptr.add(heap_head as usize) }.align_offset(align as usize) as u64;

        let aligned_head = heap_head + aligned_head_amount;

        let aligned_offset = self.bytes_allocated + aligned_head_amount;
        let wrapped_offset = {
            let aligned_start = self.heap_ptr.align_offset(align as usize) as u64;

            self.bytes_allocated + (self.capacity - heap_head) + aligned_start
        };

        let alloc_offset = match heap_head.cmp(&heap_tail) {
            std::cmp::Ordering::Greater => {
                if aligned_head + aligned_size <= self.capacity {
                    // [   free   |   used   |  <here> ]
                    //            ^tail      ^head
                    Ok(aligned_offset)
                } else if heap_tail >= aligned_size {
                    // [  <here>  |   used   |   <wrap>  ]
                    //            ^tail      ^head
                    Ok(wrapped_offset)
                } else {
                    Err(Error::OutOfMemory)
                }
            }
            std::cmp::Ordering::Less => {
                if heap_tail - aligned_head >= aligned_size {
                    // [   used   |  <here>  |   used  ]
                    //            ^head      ^tail
                    Ok(aligned_offset)
                } else {
                    Err(Error::OutOfMemory)
                }
            }
            std::cmp::Ordering::Equal => {
                if self.bytes_allocated == self.bytes_deallocated {
                    if aligned_head + aligned_size <= self.capacity {
                        // [   free       |     <here>      ]
                        //                ^head/tail
                        self.bytes_deallocated += aligned_head_amount;
                        Ok(aligned_offset)
                    } else {
                        // [   <here>       |     free      ]
                        //                  ^head/tail
                        self.bytes_deallocated = wrapped_offset;
                        Ok(wrapped_offset)
                    }
                } else {
                    Err(Error::OutOfMemory)
                }
            }
        }?;

        let alloc = Allocation {
            size: aligned_size as u32,
            alloc_id: self.allocations_served,
            offset: alloc_offset,
        };

        self.bytes_allocated = alloc_offset + aligned_size;
        self.allocations_served += 1;

        debug_assert!((alloc_offset % self.capacity) + aligned_size <= self.capacity);
        let heap_data = unsafe {
            std::slice::from_raw_parts_mut(
                self.heap_ptr.add(alloc_offset as usize).cast(),
                aligned_size as usize,
            )
        };

        Ok((alloc, heap_data))
    }

    /// Allocates enough memory to store the given data and copies it into the allocated region.
    pub fn upload<T>(&mut self, data: &[T], align: u64) -> Result<Allocation, Error> {
        let size = std::mem::size_of_val(data) as u64;
        let (allocation, heap_data) = self.allocate(size, align)?;

        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr().cast(), heap_data.as_mut_ptr(), data.len());
        }

        Ok(allocation)
    }

    /// Frees an allocation.
    ///
    /// Allocations must be freed in the same order they were allocated.
    ///
    /// ## Panics
    ///
    /// This function will panic if an earlier allocation has not been freed.
    /// This is a bug in the calling code.
    pub fn free(&mut self, allocation: Allocation) {
        assert_eq!(allocation.alloc_id, self.allocations_freed);
        self.allocations_freed += 1;
        self.bytes_deallocated = allocation.offset + allocation.size as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_heap() {
        let mut data = [0u8; 129];
        let mut allocator = RingAllocator::new(128, (&mut data[1..]).as_mut_ptr().cast());

        let (a, a_) = allocator.allocate(10, 64).unwrap();
        assert_eq!(a.size, 64);
        assert_eq!(a.alloc_id, 0);
        assert_eq!(a_.as_ptr().align_offset(64), 0);
        assert_eq!(a_.len(), 64);

        let (b, b_) = allocator.allocate(10, 8).unwrap();
        assert_eq!(b.size, 16);
        assert_eq!(b.alloc_id, 1);
        assert_eq!(b_.as_ptr().align_offset(8), 0);
        assert_eq!(b_.len(), 16);

        assert!(
            !allocator.is_full(),
            "allocated 80 bytes out of 128, allocator.is_full() should be false"
        );

        {
            let bytes_allocated = allocator.bytes_allocated;
            let bytes_deallocated = allocator.bytes_deallocated;
            let allocations_served = allocator.allocations_served;
            let allocations_freed = allocator.allocations_freed;

            assert_eq!(allocator.allocate(1, 64), Err(Error::OutOfMemory));

            assert_eq!(
                allocator.bytes_allocated, bytes_allocated,
                "failed allocation cannot change allocator state"
            );
            assert_eq!(
                allocator.bytes_deallocated, bytes_deallocated,
                "failed allocation cannot change allocator state"
            );
            assert_eq!(
                allocator.allocations_served, allocations_served,
                "failed allocation cannot change allocator state"
            );
            assert_eq!(
                allocator.allocations_freed, allocations_freed,
                "failed allocation cannot change allocator state"
            );
        }

        allocator.free(a);
        assert!(!allocator.is_free());

        allocator.free(b);
        assert!(
            allocator.is_free(),
            "allocator must be free after all allocations are freed"
        );

        // wrapping allocation
        let (c, c_) = allocator.allocate(128, 1).unwrap();
        assert_eq!(c.size, 128);
        assert_eq!(c.alloc_id, 2);
        assert_eq!(c_.as_ptr().align_offset(1), 0);
        assert_eq!(c_.len(), 128);
        assert!(allocator.is_full());

        allocator.free(c);
        assert!(allocator.is_free());
    }
}
