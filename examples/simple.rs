use tracing_subscriber::prelude::*;

use std::alloc::System;
use tracing_allocations::TracingAllocator;

#[global_allocator]
static GLOBAL: TracingAllocator<System> = TracingAllocator::new(System);

fn main() {
    let _guard = tracing_allocations::housekeeping();

    tracing_allocations::disable_in_scope(|| {
        tracing_subscriber::registry()
            .with(tracing_subscriber::filter::EnvFilter::new("trace"))
            .with(tracing_subscriber::fmt::layer())
            .init()
    });

    tracing::info_span!("foo").in_scope(|| {
        let a = Box::new([0u8; 1]);

        let c = tracing::info_span!("bar").in_scope(|| {
            drop(a);

            tracing::info_span!("baz").in_scope(|| {
                let b = Box::new([0u8; 2]);
                drop(b);
            });

            Box::new([0u8; 3])
        });

        drop(c);
    });
}
