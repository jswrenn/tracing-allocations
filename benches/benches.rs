use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{self, Write};
use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Bencher, Criterion};
use tracing_allocations::{trace_allocations, TracingAllocator};

fn no_op_writer() -> impl Write {
    struct NoOpWriter;

    impl Write for NoOpWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    NoOpWriter
}

fn bench_no_tracing(b: &mut Bencher, allocator: &dyn GlobalAlloc, layout: Layout) {
    b.iter(|| unsafe {
        let ptr = black_box(allocator.alloc(layout));
        allocator.dealloc(ptr, layout);
    });
}

fn bench_tracing(b: &mut Bencher, allocator: &dyn GlobalAlloc, layout: Layout) {
    b.iter(|| unsafe {
        trace_allocations(|| {
            let ptr = black_box(allocator.alloc(layout));
            allocator.dealloc(ptr, layout);
        })
    });
}

fn criterion_benchmark(c: &mut Criterion) {
    let filter =
        tracing_subscriber::EnvFilter::try_new("TRACE").expect("invalid tracing directive");

    tracing_subscriber::fmt()
        .with_writer(no_op_writer)
        .with_env_filter(filter)
        .init();

    const LAYOUT: Layout = Layout::new::<[String; 128]>();

    const SYSTEM_ALLOCATOR: System = System;
    const TRACING_ALLOCATOR: TracingAllocator<System> = TracingAllocator::new(System);

    let mut group = c.benchmark_group("allocation without actual tracing");

    group.bench_function("system allocator, no tracing", |b| {
        // This simply measures the overhead of using the system allocator normally.
        bench_no_tracing(b, &SYSTEM_ALLOCATOR, LAYOUT)
    });

    group.bench_function("system allocator, pointless tracing", |b| {
        // This simply measures the overhead of using the system allocator normally.
        bench_tracing(b, &SYSTEM_ALLOCATOR, LAYOUT)
    });

    group.bench_function("tracing allocator, no tracing", |b| {
        bench_no_tracing(b, &TRACING_ALLOCATOR, LAYOUT)
    });

    group.finish();

    c.bench_function("tracing allocator, tracing", |b| {
        bench_tracing(b, &TRACING_ALLOCATOR, LAYOUT)
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .significance_level(0.02)
        .noise_threshold(0.05)
        .measurement_time(Duration::from_secs(30))
        .warm_up_time(Duration::from_secs(10));
    targets = criterion_benchmark
);
criterion_main!(benches);
