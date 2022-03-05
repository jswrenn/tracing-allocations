use tracing::trace_span;
use tracing_subscriber::prelude::*;

use std::alloc::System;
use tracing_allocations::{ignore_allocations, trace_allocations, Instrument, TracingAllocator};

#[global_allocator]
static GLOBAL: TracingAllocator<System> = TracingAllocator::new(System);

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    async {
        // TRACE foo: tracing_allocations: alloc addr=94253372165520 size=1
        let a = Box::new([0u8; 1]);

        let c = ignore_allocations(move || {
            // no trace emitted
            drop(a);

            trace_allocations(|| {
                // TRACE foo: tracing_allocations: alloc addr=94253372165520 size=2
                let b = Box::new([0u8; 2]);
                // TRACE foo: tracing_allocations: dealloc addr=94253372165520 size=2
                drop(b);
            });

            // no trace emitted
            Box::new([0u8; 3])
        });

        // TRACE foo: tracing_allocations: dealloc addr=94253372165520 size=3
        drop(c);
    }
    .instrument(trace_span!("foo"))
    .await;
}
