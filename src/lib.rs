use core::mem;
use core::ops::DerefMut;

use core::{
    alloc::{GlobalAlloc, Layout},
    cell::RefCell,
};

thread_local! {
    /// Flag controlling whether to emit tracing events for allocations/deallocations on this thread.
    pub static TRACE_ALLOCATOR: RefCell<bool> = RefCell::new(false);
}

/// An allocator that emits tracing events.
///
/// ## Usage
/// ```
/// use std::alloc::System;
/// use tracing_allocations::TracingAllocator;
///
/// #[global_allocator]
/// static ALLOCATOR: TracingAllocator<System> = TracingAllocator::new(System);
/// ```
#[non_exhaustive]
pub struct TracingAllocator<A> {
    pub allocator: A,
}

impl<A> TracingAllocator<A> {
    /// Constructs a tracing allocator.
    ///
    /// ## Usage
    /// ```
    /// use std::alloc::System;
    /// use tracing_allocations::TracingAllocator;
    ///
    /// #[global_allocator]
    /// static ALLOCATOR: TracingAllocator<System> = TracingAllocator::new(System);
    /// ```
    pub const fn new(allocator: A) -> Self {
        Self { allocator }
    }
}

unsafe impl<A> GlobalAlloc for TracingAllocator<A>
where
    A: GlobalAlloc,
{
    #[track_caller]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = self.allocator.alloc(layout);
        // trace the allocation
        let _ = TRACE_ALLOCATOR.try_with(|guard| {
            // `guard.try_borrow_mut()` prevents us from tracing our traces
            if let Ok(mut trace_allocations) = guard.try_borrow_mut() {
                if mem::replace(trace_allocations.deref_mut(), false) {
                    tracing::trace! {
                        addr = ptr as usize,
                        size = layout.size(),
                        "alloc",
                    };
                    *trace_allocations = true;
                }
                drop(trace_allocations);
            }
        });
        ptr
    }

    #[track_caller]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.allocator.dealloc(ptr, layout);
        let _ = TRACE_ALLOCATOR.try_with(|guard| {
            // `guard.try_borrow_mut()` prevents us from tracing our traces
            if let Ok(mut trace_allocations) = guard.try_borrow_mut() {
                if mem::replace(trace_allocations.deref_mut(), false) {
                    tracing::trace! {
                        addr = ptr as usize,
                        size = layout.size(),
                        "dealloc",
                    };
                    *trace_allocations = true;
                }
                drop(guard);
            }
        });
    }
}

/// Trace allocations occurring within `f`.
pub fn trace_allocations<F: FnOnce() -> R, R>(f: F) -> R {
    TRACE_ALLOCATOR.with(|guard| {
        let mut previous_state = false;
        if let Ok(mut trace_allocations) = guard.try_borrow_mut() {
            previous_state = mem::replace(&mut trace_allocations, true);
        }
        let res = f();
        if let Ok(mut trace_allocations) = guard.try_borrow_mut() {
            *trace_allocations = previous_state;
        }
        res
    })
}

/// Do not trace allocations occurring within `f`.
pub fn ignore_allocations<F: FnOnce() -> R, R>(f: F) -> R {
    TRACE_ALLOCATOR.with(|guard| {
        let mut previous_state = true;
        if let Ok(mut trace_allocations) = guard.try_borrow_mut() {
            previous_state = mem::replace(&mut trace_allocations, false);
        }
        let res = f();
        if let Ok(mut trace_allocations) = guard.try_borrow_mut() {
            *trace_allocations = previous_state;
        }
        res
    })
}
