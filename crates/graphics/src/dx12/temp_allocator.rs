#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The heap does not have enough free memory to satisfy the allocation
    /// request.
    OutOfMemory,
    /// The heap is not large enough to satisfy the allocation request.
    InsufficientCapacity,
}

pub struct FrameAllocator<'a> {
    bytes_allocated_at_start: u64,
    bytes_allocated: u64,
    allocator: &'a mut Allocator,
}

impl<'a> FrameAllocator<'a> {
    pub fn new(allocator: &'a mut Allocator) -> Self {
        Self {
            bytes_allocated_at_start: allocator.bytes_allocated,
            bytes_allocated: allocator.bytes_allocated,
            allocator,
        }
    }

    /// Allocates a block of memory from the heap.
    ///
    /// If the request returns `Error::OutOfMemory`,
    pub fn allocate(&mut self, size: u64, alignment: u64) -> Result<Allocation, Error> {
        let tail_ptr = self.allocator.bytes_freed % self.allocator.capacity;
        let base_ptr = self.bytes_allocated % self.allocator.capacity;
        let aligned_ptr = next_multiple_of(base_ptr, alignment);

        if size > self.allocator.capacity {
            return Err(Error::InsufficientCapacity);
        }

        enum Adjust {
            Align,
            Wrap,
        }

        let adjust_amount = match tail_ptr.cmp(&base_ptr) {
            std::cmp::Ordering::Less => {
                // [      free      |    used    |   free   ]
                //                  ^tail_ptr    ^base_ptr
                if self.allocator.capacity - aligned_ptr >= size {
                    Some(Adjust::Align)
                } else if tail_ptr >= size {
                    Some(Adjust::Wrap)
                } else {
                    None
                }
            }
            std::cmp::Ordering::Equal => {
                // [      free      |      free      ]
                //                  ^base_ptr/tail_ptr
                if self.allocator.capacity - aligned_ptr >= size {
                    Some(Adjust::Align)
                } else {
                    Some(Adjust::Wrap)
                }
            }
            std::cmp::Ordering::Greater => {
                // [    used    |   free   |    used    ]
                //              ^base_ptr  ^tail_ptr
                if tail_ptr - aligned_ptr > size {
                    Some(Adjust::Align)
                } else {
                    None
                }
            }
        }
        .ok_or(Error::OutOfMemory)?;

        let adjust_amount = match adjust_amount {
            Adjust::Align => aligned_ptr - base_ptr,
            Adjust::Wrap => self.allocator.capacity - base_ptr,
        };

        let r = Ok(Allocation {
            size,
            offset: self.bytes_allocated + adjust_amount,
        });

        self.bytes_allocated += adjust_amount + size;

        r
    }

    pub fn finish(mut self) -> FrameMarker {
        self.allocator.bytes_allocated = self.bytes_allocated;

        FrameMarker {
            end: self.bytes_allocated,
            start: self.bytes_allocated_at_start,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FrameMarker {
    end: u64,
    start: u64,
}

impl PartialEq for FrameMarker {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start
    }
}

impl PartialOrd for FrameMarker {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.start.cmp(&other.start))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Allocation {
    pub size: u64,
    pub offset: u64,
}

pub struct Allocator {
    capacity: u64,
    bytes_freed: u64,
    bytes_allocated: u64,
}

impl Allocator {
    pub fn new(capacity: u64) -> Self {
        Self {
            capacity,
            bytes_freed: 0,
            bytes_allocated: 0,
        }
    }

    pub fn bytes_free(&self) -> u64 {
        self.capacity - (self.bytes_allocated - self.bytes_freed)
    }

    pub fn begin_frame(&mut self) -> FrameAllocator {
        FrameAllocator::new(self)
    }

    pub fn free_frame(&mut self, marker: FrameMarker) {
        assert!(marker.start == self.bytes_freed);
        self.bytes_freed += marker.end - marker.start;
    }

    #[cfg(test)]
    fn debug_print(&mut self, markers: &[FrameMarker]) {
        let mut line = [" "; 100];

        for marker in markers {
            let m_start = ((marker.start % self.capacity) as f64 / self.capacity as f64
                * line.len() as f64)
                .floor() as usize;
            let m_end = ((marker.end % self.capacity) as f64 / self.capacity as f64
                * line.len() as f64)
                .ceil() as usize;

            if m_start < m_end {
                for c in &mut line[m_start..m_end] {
                    *c = "#";
                }
            } else {
                for c in &mut line[m_start..] {
                    *c = "#";
                }
                for c in &mut line[..m_end] {
                    *c = "#";
                }
            }
        }

        let m_free = ((self.bytes_freed % self.capacity) as f64 / self.capacity as f64
            * line.len() as f64)
            .floor() as usize;
        let m_alloc = ((self.bytes_allocated % self.capacity) as f64 / self.capacity as f64
            * line.len() as f64)
            .floor() as usize;

        if m_free == m_alloc {
            line[m_free] = "x";
        } else {
            line[m_free] = "f";
            line[m_alloc] = "a";
        }

        println!("[{}]", line.join(""));
    }
}

fn next_multiple_of(a: u64, b: u64) -> u64 {
    match a % b {
        0 => a,
        r => a + b - r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let mut allocator = Allocator::new(100);

        let m0 = {
            let mut frame = allocator.begin_frame();
            assert_eq!(
                // case 3
                frame.allocate(20, 4),
                Ok(Allocation {
                    size: 20,
                    offset: 0
                })
            );
            frame.finish()
        };

        assert_eq!(m0, FrameMarker { end: 20, start: 0 });
        allocator.debug_print(&[m0]);

        let m1 = {
            let mut frame = allocator.begin_frame();
            assert_eq!(
                // case 1
                frame.allocate(70, 4),
                Ok(Allocation {
                    size: 70,
                    offset: 20
                })
            );
            frame.finish()
        };

        assert_eq!(m1, FrameMarker { end: 90, start: 20 });
        allocator.free_frame(m0);
        allocator.debug_print(&[m1]);

        let m2 = {
            let mut frame = allocator.begin_frame();
            assert_eq!(
                // cas 2
                frame.allocate(20, 8),
                Ok(Allocation {
                    size: 20,
                    offset: 100
                })
            );
            frame.finish()
        };

        assert_eq!(
            m2,
            FrameMarker {
                end: 120,
                start: 90
            }
        );
        allocator.free_frame(m1);
        allocator.debug_print(&[m2]);

        let m3 = {
            let mut frame = allocator.begin_frame();
            assert_eq!(
                // case 5
                frame.allocate(15, 64).unwrap(),
                Allocation {
                    size: 15,
                    offset: 164
                }
            );
            frame.finish()
        };

        assert_eq!(
            m3,
            FrameMarker {
                end: 179,
                start: 120
            }
        );
        allocator.free_frame(m2);
        allocator.debug_print(&[m3]);
        allocator.free_frame(m3);

        let m4 = {
            let mut frame = allocator.begin_frame();
            assert_eq!(
                // case 4
                frame.allocate(80, 16),
                Ok(Allocation {
                    size: 80,
                    offset: 200
                })
            );
            frame.finish()
        };

        assert_eq!(
            m4,
            FrameMarker {
                end: 280,
                start: 179
            }
        );
        allocator.debug_print(&[m4]);
    }
}
