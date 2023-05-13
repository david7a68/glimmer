use std::ffi::c_void;

use crate::backend::next_multiple_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    OutOfMemory,
}

pub struct LinearAllocator {
    capacity: u64,
    heap_ptr: *mut c_void,
    bytes_allocated: u64,
}

impl LinearAllocator {
    pub fn new(capacity: u64, heap_ptr: *mut c_void) -> Self {
        Self {
            capacity,
            heap_ptr,
            bytes_allocated: 0,
        }
    }

    pub fn bytes_allocated(&self) -> u64 {
        self.bytes_allocated
    }

    pub fn is_full(&self) -> bool {
        self.bytes_allocated == self.capacity
    }

    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    pub fn can_fit(&self, size: u64, align: u64) -> bool {
        self.bytes_allocated + self.alloc_size(size, align).0 <= self.capacity
    }

    pub fn allocate(&mut self, size: u64, align: u64) -> Result<(u64, &mut [u8]), Error> {
        let (alloc_size, align_amount) = self.alloc_size(size, align);

        if self.bytes_allocated + alloc_size > self.capacity {
            return Err(Error::OutOfMemory);
        } else {
            let offset = self.bytes_allocated + align_amount;
            let ptr = unsafe { self.heap_ptr.add(offset as usize) };
            self.bytes_allocated += alloc_size;

            Ok((offset, unsafe {
                std::slice::from_raw_parts_mut(ptr.cast(), size as usize)
            }))
        }
    }

    pub fn clear(&mut self) {
        self.bytes_allocated = 0;
    }

    fn alloc_size(&self, size: u64, align: u64) -> (u64, u64) {
        let align_amount = unsafe { self.heap_ptr.add(self.bytes_allocated as usize) }
            .align_offset(align as usize) as u64;

        (align_amount + next_multiple_of(size, align), align_amount)
    }
}
