//! Memory management implementation
//!
//! SV39 page-based virtual-memory architecture for RV64 systems, and
//! everything about memory management, like frame allocator, page table,
//! map area and memory set, is implemented here.
//!
//! Every task or process has a memory_set to control its virtual memory.

mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;

pub use address::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
use address::{StepByOne, VPNRange};
use alloc::string::String;
pub use frame_allocator::{frame_alloc, FrameTracker};
pub use memory_set::remap_test;
pub use memory_set::{MapPermission, MemorySet, KERNEL_SPACE};
pub use page_table::{translated_byte_buffer, translated_refmut, translated_str, PageTableEntry};
use page_table::{PTEFlags, PageTable};

use crate::task::current_task;

/// initiate heap allocator, frame allocator and kernel space
pub fn init() {
    heap_allocator::init_heap();
    frame_allocator::init_frame_allocator();
    KERNEL_SPACE.exclusive_access().activate();
}

pub fn memeory_map(
    start_va: VirtAddr,
    end_va: VirtAddr,
    map_perm: MapPermission,
) -> Result<(), String> {
    let current_task = current_task().unwrap();
    let mut inner = current_task.inner_exclusive_access();
    inner
        .memory_set
        .insert_framed_area_result(start_va, end_va, map_perm)
}

pub fn memeory_unmap(start_va: VirtAddr, end_va: VirtAddr) -> Result<(), String> {
    let current_task = current_task().unwrap();
    let mut inner = current_task.inner_exclusive_access();
    inner.memory_set.remove_area_result(start_va, end_va)
}
