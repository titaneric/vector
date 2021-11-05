use buffers::{self, WhenFull};
use criterion::{
    criterion_group, criterion_main, measurement::WallTime, BatchSize, BenchmarkGroup, BenchmarkId,
    Criterion, SamplingMode, Throughput,
};
use std::mem;
use std::time::Duration;
use tokio::runtime::Runtime;

use crate::common::{init_instrumentation, war_measurement, wtr_measurement};

mod common;

macro_rules! experiment {
    ($criterion:expr, [$( $width:expr ),*], $group_name:expr, $id_slug:expr, $measure_fn:ident) => {
        let rt = Runtime::new().unwrap();
        rt.block_on(async move {
            let mut group: BenchmarkGroup<WallTime> = $criterion.benchmark_group($group_name);
            group.sampling_mode(SamplingMode::Auto);
            init_instrumentation();

            let max_events = 1_000;
            $(
                let bytes = mem::size_of::<crate::common::Message<$width>>();
                group.throughput(Throughput::Elements(max_events as u64));
                group.bench_with_input(
                    BenchmarkId::new($id_slug, bytes),
                    &max_events,
                    |b, max_events| {
                        b.iter_batched(
                            || {
                                crate::common::setup_in_memory_v2::<$width>(*max_events, WhenFull::DropNewest)
                            },
                            $measure_fn,
                            BatchSize::SmallInput,
                        )
                    },
                );
            )*
        });
    }
}

//
// [MEMORY] Write Then Read benchmark
//
// This benchmark uses the in-memory buffer with a sender/receiver that fully
// write all messages into the buffer, then fully read all messages. DropNewest
// is in effect when full condition is hit but sizes are carefully chosen to
// never fill the buffer.
//

fn write_then_read_in_memory_v2(c: &mut Criterion) {
    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-v2-memory",
        "write-then-read",
        wtr_measurement
    );
}

//
// [MEMORY] Write And Read benchmark
//
// This benchmark uses the in-memory buffer with a sender/receiver that write
// and read in lockstep. DropNewest is in effect when full condition is hit but
// sizes are carefully chosen to never fill the buffer.
//

fn write_and_read_in_memory_v2(c: &mut Criterion) {
    experiment!(
        c,
        [32, 64, 128, 256, 512, 1024],
        "buffer-v2-memory",
        "write-and-read",
        war_measurement
    );
}

criterion_group!(
    name = in_memory_v2;
    config = Criterion::default().measurement_time(Duration::from_secs(120)).confidence_level(0.99).nresamples(500_000).sample_size(250);
    targets = write_and_read_in_memory_v2, write_then_read_in_memory_v2
);
criterion_main!(in_memory_v2);
