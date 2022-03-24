//! A global allocator that emits tracing events.
//!
//! See [`TracingAllocator`] for more information.
//!
//! ## Usage
//! ```
//! use std::alloc::System;
//! use tracing_allocations::TracingAllocator;
//!
//! #[global_allocator]
//! static ALLOCATOR: TracingAllocator<System> = TracingAllocator::new(System);
//!
//! fn main() {
//!     let _guard = tracing_allocations::housekeeping();
//!     /* your code here */
//! }
//! ```

use core::{
    alloc::{GlobalAlloc, Layout},
    cell::{RefCell, RefMut},
};

use std::panic::catch_unwind;

/// A global allocator that emits tracing events.
///
/// This allocator emits [`TRACE`]-level events. See method documentation for
/// more information:
/// - [`TracingAllocator::alloc`]
/// - [`TracingAllocator::dealloc`]
/// - [`TracingAllocator::alloc_zeroed`]
/// - [`TracingAllocator::realloc`]
///
/// [`TRACE`]: tracing::Level::TRACE
#[non_exhaustive]
pub struct TracingAllocator<A> {
    /// The underlying allocator, which `TracingAllocator` delegates allocations
    /// and deallocations to.
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
    ///
    /// fn main() {
    ///     let _guard = tracing_allocations::housekeeping();
    ///     /* your code here */
    /// }
    /// ```
    pub const fn new(allocator: A) -> Self {
        Self { allocator }
    }
}

/// **Call this function at the start of `main`.**
///
/// This routine performs housekeeping tasks that help you avoid deadlocking
/// your application, or panicking when Rust performs cleanup tasks at the end
/// of your program.
///
/// ## Usage
/// ```
/// use std::alloc::System;
/// use tracing_allocations::TracingAllocator;
///
/// #[global_allocator]
/// static ALLOCATOR: TracingAllocator<System> = TracingAllocator::new(System);
///
/// fn main() {
///     let _guard = tracing_allocations::housekeeping();
///     /* your code here */
/// }
/// ```
///
/// ## Details
/// When invoked, this function [temporarily disables allocation
/// tracing][disable_in_scope], then invokes [`std::io::stdout()`] to initialize
/// [`std::io::Stdout`]'s shared, global buffer. Doing so helps prevent a
/// potential source of deadlocks: If the initilization of `Stdout` occurs
/// *after* allocation tracing is enabled, and the tracing subscriber
/// consequently attempts prints to stdout, that attempt to output will
/// deadlock.
///
/// If you are aware of other types in the standard library that pose similar
/// risks, please [file an issue][issue-tracker]. If your application uses types
/// outside the standard library that pose such an issue, you can safely
/// initialize them with [`disable_in_scope`].
///
/// When dropped, the guard produced by this function disables allocation on the
/// current thread for the remainder of the program's execution. This avoids a
/// potential panic that can occur *after* `main` (see [rust-lang/rust#95126]).
///
/// [issue-tracker]: https://github.com/jswrenn/tracing-allocations
/// [rust-lang/rust#95126]: https://github.com/rust-lang/rust/issues/95126
#[must_use]
pub fn housekeeping() -> impl Drop {
    use core::marker::PhantomData;

    struct Guard(PhantomData<*mut ()>);

    impl Drop for Guard {
        fn drop(&mut self) {
            // disable tracing so `std::io::cleanup()` doesn't panic
            maybe_with_guard(|mut trace| *trace = false);
        }
    }

    disable_in_scope(|| {
        let _ = std::io::stdout();
        Guard(PhantomData)
    })
}

thread_local! {
    /// Flag controlling whether to emit tracing events for allocation-related
    /// routines on this thread.
    static TRACE_ALLOCATOR: RefCell<bool> = RefCell::new(true);
}

/// Run the given function with allocation tracing disabled on the current
/// thread.
pub fn disable_in_scope<F, R>(f: F) -> R
where
    F: FnOnce() -> R
{
    let prev = TRACE_ALLOCATOR.try_with(|guard| {
        guard.replace(false)
    }).unwrap_or(false);

    let res = f();

    let _ = TRACE_ALLOCATOR.try_with(|guard| {
        guard.replace(prev)
    });

    res
}

fn maybe_with_guard<F>(f: F)
where
    F: for<'a> FnOnce(RefMut<'a, bool>),
{
    let _ = TRACE_ALLOCATOR.try_with(|guard| guard.try_borrow_mut().map(f));
}

unsafe impl<A> GlobalAlloc for TracingAllocator<A>
where
    A: GlobalAlloc,
{
    /// Allocate memory as described by the given `layout`.
    /// [Read more.][GlobalAlloc::alloc]
    ///
    /// Emits [`TRACE`]-level events with the following metadata:
    /// - **`name`**  
    ///   "alloc"
    /// - **`target`**  
    ///   "tracing::allocator"
    /// - **`addr`: [`usize`]**  
    ///   the address of the allocation
    /// - **`size`: [`usize`]**  
    ///   the size of the allocation
    ///
    /// [`TRACE`]: tracing::Level::TRACE
    #[track_caller]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = self.allocator.alloc(layout);

        // safety: global allocators must not unwind
        let _ = catch_unwind(|| {
            maybe_with_guard(|trace_allocations| {
                if *trace_allocations {
                    tracing::trace! {
                        addr = ptr as usize,
                        size = layout.size(),
                        "alloc",
                    };
                }
            })
        });

        ptr
    }

    /// Deallocate the block of memory at the given `ptr` pointer with the given
    /// `layout`.
    /// [Read more.][GlobalAlloc::dealloc]
    ///
    /// Emits [`TRACE`]-level events with the following metadata:
    /// - **`name`**  
    ///   "dealloc"
    /// - **`target`**  
    ///   "tracing::allocator"
    /// - **`addr`: [`usize`]**  
    ///   the address of the deallocation
    /// - **`size`: [`usize`]**  
    ///   the size of the deallocation
    ///
    /// [`TRACE`]: tracing::Level::TRACE
    #[track_caller]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.allocator.dealloc(ptr, layout);

        // safety: global allocators must not unwind
        let _ = catch_unwind(|| {
            maybe_with_guard(|trace_allocations| {
                if *trace_allocations {
                    tracing::trace! {
                        addr = ptr as usize,
                        size = layout.size(),
                        "dealloc",
                    };
                }
            })
        });
    }

    /// Behaves like `alloc`, but also ensures that the contents are set to zero
    /// before being returned.
    /// [Read more.][GlobalAlloc::alloc_zeroed]
    ///
    /// Emits [`TRACE`]-level events with the following metadata:
    /// - **`name`**  
    ///   "alloc_zeroed"
    /// - **`target`**  
    ///   "tracing::allocator"
    /// - **`addr`: [`usize`]**  
    ///   the address of the allocation
    /// - **`size`: [`usize`]**  
    ///   the size of the allocation
    ///
    /// [`TRACE`]: tracing::Level::TRACE
    #[track_caller]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = self.allocator.alloc_zeroed(layout);

        // safety: global allocators must not unwind
        let _ = catch_unwind(|| {
            maybe_with_guard(|trace_allocations| {
                if *trace_allocations {
                    tracing::trace! {
                        addr = ptr as usize,
                        size = layout.size(),
                        "alloc_zeroed",
                    }
                }
            })
        });

        ptr
    }

    /// Shrink or grow a block of memory to the given `new_size`. The block is
    /// described by the given `old_ptr` pointer and `old_layout` layout.
    /// [Read more.][GlobalAlloc::realloc]
    ///
    /// Emits [`TRACE`]-level events with the following metadata:
    /// - **`name`**  
    ///   "realloc"
    /// - **`target`**  
    ///   "tracing::allocator"
    /// - **`old_addr`: [`usize`]**  
    ///   the address of the existing allocation
    /// - **`old_size`: [`usize`]**  
    ///   the size of the existing allocation
    /// - **`new_addr`: [`usize`]**  
    ///   the address of the new allocation
    /// - **`new_size`: [`usize`]**  
    ///   the size of the new allocation
    ///
    /// [`TRACE`]: tracing::Level::TRACE
    #[track_caller]
    unsafe fn realloc(&self, old_ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = self.allocator.realloc(old_ptr, old_layout, new_size);

        // safety: global allocators must not unwind
        let _ = catch_unwind(|| {
            maybe_with_guard(|trace_allocations| {
                if *trace_allocations {
                    tracing::trace! {
                        old_addr = old_ptr as usize,
                        old_size = old_layout.size(),
                        new_addr = new_ptr as usize,
                        new_size = new_size,
                        "realloc",
                    }
                }
            })
        });

        new_ptr
    }
}
