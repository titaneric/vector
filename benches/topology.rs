use criterion::{criterion_group, BatchSize, Criterion, SamplingMode, Throughput};

use futures::{compat::Future01CompatExt, future, stream, StreamExt};
use rand::{rngs::SmallRng, thread_rng, Rng, SeedableRng};

use vector::{
    config, sinks, sources,
    test_util::{
        next_addr, random_lines, runtime, send_lines, start_topology, wait_for_tcp, CountReceiver,
    },
    transforms,
};

fn benchmark_simple_pipes(c: &mut Criterion) {
    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut group = c.benchmark_group("pipe");
    group.sampling_mode(SamplingMode::Flat);

    let benchmarks = [
        ("simple", 10_000, 100, 1),
        ("small_lines", 10_000, 1, 1),
        ("big_lines", 2_000, 10_000, 1),
        ("multiple_writers", 1_000, 100, 10),
    ];

    for (name, num_lines, line_size, num_writers) in benchmarks.iter() {
        group.throughput(Throughput::Bytes((num_lines * line_size) as u64));
        group.bench_function(format!("pipe_{}", name), |b| {
            b.iter_batched(
                || {
                    let mut config = config::Config::builder();
                    config.add_source(
                        "in",
                        sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
                    );
                    config.add_sink(
                        "out",
                        &["in"],
                        sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                            out_addr.to_string(),
                        ),
                    );

                    let mut rt = runtime();
                    let (output_lines, topology) = rt.block_on(async move {
                        let output_lines = CountReceiver::receive_lines(out_addr);
                        let (topology, _crash) =
                            start_topology(config.build().unwrap(), false).await;
                        wait_for_tcp(in_addr).await;
                        (output_lines, topology)
                    });
                    (rt, topology, output_lines)
                },
                |(mut rt, topology, output_lines)| {
                    rt.block_on(async move {
                        let sends = stream::iter(0..*num_writers)
                            .map(|_| {
                                let lines = random_lines(*line_size).take(*num_lines);
                                send_lines(in_addr, lines)
                            })
                            .collect::<Vec<_>>()
                            .await;
                        future::try_join_all(sends).await.unwrap();

                        topology.stop().compat().await.unwrap();

                        let output_lines = output_lines.await;

                        debug_assert_eq!(*num_lines * num_writers, output_lines.len());

                        output_lines
                    });
                },
                BatchSize::PerIteration,
            );
        });
    }

    group.finish();
}

fn benchmark_interconnected(c: &mut Criterion) {
    let num_lines: usize = 10_000;
    let line_size: usize = 100;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr1 = next_addr();
    let out_addr2 = next_addr();

    let mut group = c.benchmark_group("interconnected");
    group.throughput(Throughput::Bytes((num_lines * line_size * 2) as u64));
    group.sampling_mode(SamplingMode::Flat);

    group.bench_function("interconnected", |b| {
        b.iter_batched(
            || {
                let mut config = config::Config::builder();
                config.add_source(
                    "in1",
                    sources::socket::SocketConfig::make_basic_tcp_config(in_addr1),
                );
                config.add_source(
                    "in2",
                    sources::socket::SocketConfig::make_basic_tcp_config(in_addr2),
                );
                config.add_sink(
                    "out1",
                    &["in1", "in2"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr1.to_string()),
                );
                config.add_sink(
                    "out2",
                    &["in1", "in2"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr2.to_string()),
                );

                let mut rt = runtime();
                let (output_lines1, output_lines2, topology) = rt.block_on(async move {
                    let output_lines1 = CountReceiver::receive_lines(out_addr1);
                    let output_lines2 = CountReceiver::receive_lines(out_addr2);
                    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
                    wait_for_tcp(in_addr1).await;
                    wait_for_tcp(in_addr2).await;
                    (output_lines1, output_lines2, topology)
                });
                (rt, topology, output_lines1, output_lines2)
            },
            |(mut rt, topology, output_lines1, output_lines2)| {
                rt.block_on(async move {
                    let lines1 = random_lines(line_size).take(num_lines);
                    send_lines(in_addr1, lines1).await.unwrap();
                    let lines2 = random_lines(line_size).take(num_lines);
                    send_lines(in_addr2, lines2).await.unwrap();

                    topology.stop().compat().await.unwrap();

                    let output_lines1 = output_lines1.await;
                    let output_lines2 = output_lines2.await;

                    debug_assert_eq!(num_lines * 2, output_lines1.len());
                    debug_assert_eq!(num_lines * 2, output_lines2.len());

                    (output_lines1, output_lines2)
                });
            },
            BatchSize::PerIteration,
        );
    });

    group.finish();
}

fn benchmark_transforms(c: &mut Criterion) {
    let num_lines: usize = 10_000;
    let line_size: usize = 100;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut group = c.benchmark_group("transforms");
    group.throughput(Throughput::Bytes(
        (num_lines * (line_size + "status=404".len())) as u64,
    ));
    group.sampling_mode(SamplingMode::Flat);

    group.bench_function("transforms", |b| {
        b.iter_batched(
            || {
                let mut config = config::Config::builder();
                config.add_source(
                    "in",
                    sources::socket::SocketConfig::make_basic_tcp_config(in_addr),
                );
                config.add_transform(
                    "parser",
                    &["in"],
                    transforms::regex_parser::RegexParserConfig {
                        patterns: vec![r"status=(?P<status>\d+)".to_string()],
                        field: None,
                        ..Default::default()
                    },
                );
                config.add_transform(
                    "filter",
                    &["parser"],
                    transforms::field_filter::FieldFilterConfig {
                        field: "status".to_string(),
                        value: "404".to_string(),
                    },
                );
                config.add_sink(
                    "out",
                    &["filter"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
                );

                let mut rt = runtime();
                let (output_lines, topology) = rt.block_on(async move {
                    let output_lines = CountReceiver::receive_lines(out_addr);
                    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
                    wait_for_tcp(in_addr).await;
                    (output_lines, topology)
                });
                (rt, topology, output_lines)
            },
            |(mut rt, topology, output_lines)| {
                rt.block_on(async move {
                    let lines = random_lines(line_size)
                        .map(|l| l + "status=404")
                        .take(num_lines);
                    send_lines(in_addr, lines).await.unwrap();

                    topology.stop().compat().await.unwrap();

                    let output_lines = output_lines.await;

                    debug_assert_eq!(num_lines, output_lines.len());

                    output_lines
                });
            },
            BatchSize::PerIteration,
        );
    });

    group.finish();
}

fn benchmark_complex(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let sample_rate: u64 = 10;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr_all = next_addr();
    let out_addr_sampled = next_addr();
    let out_addr_200 = next_addr();
    let out_addr_404 = next_addr();
    let out_addr_500 = next_addr();

    let mut group = c.benchmark_group("complex");
    group.sampling_mode(SamplingMode::Flat);

    group.bench_function("complex", |b| {
        b.iter_batched(
            || {
                let mut config = config::Config::builder();
                config.add_source(
                    "in1",
                    sources::socket::SocketConfig::make_basic_tcp_config(in_addr1),
                );
                config.add_source(
                    "in2",
                    sources::socket::SocketConfig::make_basic_tcp_config(in_addr2),
                );
                config.add_transform(
                    "parser",
                    &["in1", "in2"],
                    transforms::regex_parser::RegexParserConfig {
                        patterns: vec![r"status=(?P<status>\d+)".to_string()],
                        drop_field: false,
                        field: None,
                        ..Default::default()
                    },
                );
                config.add_transform(
                    "filter_200",
                    &["parser"],
                    transforms::field_filter::FieldFilterConfig {
                        field: "status".to_string(),
                        value: "200".to_string(),
                    },
                );
                config.add_transform(
                    "filter_404",
                    &["parser"],
                    transforms::field_filter::FieldFilterConfig {
                        field: "status".to_string(),
                        value: "404".to_string(),
                    },
                );
                config.add_transform(
                    "filter_500",
                    &["parser"],
                    transforms::field_filter::FieldFilterConfig {
                        field: "status".to_string(),
                        value: "500".to_string(),
                    },
                );
                config.add_transform(
                    "sampler",
                    &["parser"],
                    transforms::sampler::SamplerConfig {
                        rate: sample_rate,
                        key_field: None,
                        exclude: None,
                    },
                );
                config.add_sink(
                    "out_all",
                    &["parser"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                        out_addr_all.to_string(),
                    ),
                );
                config.add_sink(
                    "out_sampled",
                    &["sampler"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                        out_addr_sampled.to_string(),
                    ),
                );
                config.add_sink(
                    "out_200",
                    &["filter_200"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                        out_addr_200.to_string(),
                    ),
                );
                config.add_sink(
                    "out_404",
                    &["filter_404"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                        out_addr_404.to_string(),
                    ),
                );
                config.add_sink(
                    "out_500",
                    &["filter_500"],
                    sinks::socket::SocketSinkConfig::make_basic_tcp_config(
                        out_addr_500.to_string(),
                    ),
                );

                let mut rt = runtime();
                let (
                    output_lines_all,
                    output_lines_sampled,
                    output_lines_200,
                    output_lines_404,
                    topology,
                ) = rt.block_on(async move {
                    let output_lines_all = CountReceiver::receive_lines(out_addr_all);
                    let output_lines_sampled = CountReceiver::receive_lines(out_addr_sampled);
                    let output_lines_200 = CountReceiver::receive_lines(out_addr_200);
                    let output_lines_404 = CountReceiver::receive_lines(out_addr_404);
                    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
                    wait_for_tcp(in_addr1).await;
                    wait_for_tcp(in_addr2).await;
                    (
                        output_lines_all,
                        output_lines_sampled,
                        output_lines_200,
                        output_lines_404,
                        topology,
                    )
                });
                (
                    rt,
                    topology,
                    output_lines_all,
                    output_lines_sampled,
                    output_lines_200,
                    output_lines_404,
                )
            },
            |(
                mut rt,
                topology,
                output_lines_all,
                output_lines_sampled,
                output_lines_200,
                output_lines_404,
            )| {
                rt.block_on(async move {
                    // One sender generates pure random lines
                    let lines1 = random_lines(100).take(num_lines);
                    send_lines(in_addr1, lines1).await.unwrap();

                    // The other includes either status=200 or status=404
                    let mut rng = SmallRng::from_rng(thread_rng()).unwrap();
                    let lines2 = random_lines(100)
                        .map(move |mut l| {
                            let status = if rng.gen_bool(0.5) { "200" } else { "404" };
                            l += "status=";
                            l += status;
                            l
                        })
                        .take(num_lines);
                    send_lines(in_addr2, lines2).await.unwrap();

                    topology.stop().compat().await.unwrap();

                    let output_lines_all = output_lines_all.await.len();
                    let output_lines_sampled = output_lines_sampled.await.len();
                    let output_lines_200 = output_lines_200.await.len();
                    let output_lines_404 = output_lines_404.await.len();

                    debug_assert_eq!(output_lines_all, num_lines * 2);
                    #[cfg(debug_assertions)]
                    {
                        use approx::assert_relative_eq;

                        // binomial distribution
                        let sample_stdev = (output_lines_all as f64
                            * (1f64 / sample_rate as f64)
                            * (1f64 - (1f64 / sample_rate as f64)))
                            .sqrt();

                        assert_relative_eq!(
                            output_lines_sampled as f64,
                            output_lines_all as f64 * (1f64 / sample_rate as f64),
                            epsilon = sample_stdev * 4f64 // should cover 99.993666% of cases
                        );
                    }
                    debug_assert!(output_lines_200 > 0);
                    debug_assert!(output_lines_404 > 0);
                    debug_assert_eq!(output_lines_200 + output_lines_404, num_lines);

                    (
                        output_lines_all,
                        output_lines_sampled,
                        output_lines_200,
                        output_lines_404,
                    )
                });
            },
            BatchSize::PerIteration,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_simple_pipes,
    benchmark_interconnected,
    benchmark_transforms,
    benchmark_complex,
);
