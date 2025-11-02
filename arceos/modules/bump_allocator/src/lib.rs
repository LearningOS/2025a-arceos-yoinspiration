#![no_std]

use allocator::{BaseAllocator, ByteAllocator, PageAllocator};

/// Early memory allocator
/// Use it before formal bytes-allocator and pages-allocator can work!
/// This is a double-end memory range:
/// - Alloc bytes forward
/// - Alloc pages backward
///
/// [ bytes-used | avail-area | pages-used ]
/// |            | -->    <-- |            |
/// start       b_pos        p_pos       end
///
/// For bytes area, 'count' records number of allocations.
/// When it goes down to ZERO, free bytes-used area.
/// For pages area, it will never be freed!
///
pub struct EarlyAllocator<const SIZE: usize> {
    start: usize,
    end: usize,
    b_pos: usize,
    p_pos: usize,
}

impl<const SIZE: usize> EarlyAllocator<SIZE> {
    pub const fn new() -> Self {
        Self {
            start: 0,
            end: 0,
            b_pos: 0,
            p_pos: 0,
        }
    }
}

impl<const SIZE: usize> BaseAllocator for EarlyAllocator<SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        self.start = start;
        self.end = start + size;
        self.b_pos = start;
        self.p_pos = self.end;
    }

    fn add_memory(&mut self, _start: usize, _size: usize) -> allocator::AllocResult {
        Err(allocator::AllocError::NoMemory)
    }
}

impl<const SIZE: usize> ByteAllocator for EarlyAllocator<SIZE> {
    fn alloc(
        &mut self,
        layout: core::alloc::Layout,
    ) -> allocator::AllocResult<core::ptr::NonNull<u8>> {
        let align = layout.align();
        let size = layout.size();
        
        // Align b_pos
        let aligned_pos = (self.b_pos + align - 1) & !(align - 1);
        
        // Check if there's enough space
        if aligned_pos + size > self.p_pos {
            return Err(allocator::AllocError::NoMemory);
        }
        
        self.b_pos = aligned_pos + size;
        
        unsafe {
            Ok(core::ptr::NonNull::new_unchecked(aligned_pos as *mut u8))
        }
    }

    fn dealloc(&mut self, _pos: core::ptr::NonNull<u8>, _layout: core::alloc::Layout) {
        // Bump allocator doesn't free
    }

    fn total_bytes(&self) -> usize {
        self.end - self.start
    }

    fn used_bytes(&self) -> usize {
        self.b_pos - self.start
    }

    fn available_bytes(&self) -> usize {
        if self.b_pos < self.p_pos {
            self.p_pos - self.b_pos
        } else {
            0
        }
    }
}

impl<const SIZE: usize> PageAllocator for EarlyAllocator<SIZE> {
    const PAGE_SIZE: usize = SIZE;

    fn alloc_pages(
        &mut self,
        num_pages: usize,
        align_pow2: usize,
    ) -> allocator::AllocResult<usize> {
        let align = 1usize << align_pow2;
        let size = num_pages * SIZE;
        
        // Align p_pos backward
        let aligned_pos = (self.p_pos - size) & !(align - 1);
        
        // Check if there's enough space
        if aligned_pos < self.b_pos {
            return Err(allocator::AllocError::NoMemory);
        }
        
        self.p_pos = aligned_pos;
        
        Ok(aligned_pos)
    }

    fn dealloc_pages(&mut self, _pos: usize, _num_pages: usize) {
        // Bump allocator doesn't free
    }

    fn total_pages(&self) -> usize {
        (self.end - self.start) / SIZE
    }

    fn used_pages(&self) -> usize {
        (self.end - self.p_pos) / SIZE
    }

    fn available_pages(&self) -> usize {
        if self.b_pos < self.p_pos {
            (self.p_pos - self.b_pos) / SIZE
        } else {
            0
        }
    }
}