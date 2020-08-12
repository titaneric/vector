#![cfg(all(
    feature = "sinks-socket",
    feature = "transforms-sampler",
    feature = "sources-socket",
))]

use approx::assert_relative_eq;
use futures::compat::Future01CompatExt;
use tokio::net::TcpListener;
use vector::{
    config,
    runtime::Runtime,
    sinks, sources,
    test_util::{
        next_addr, random_lines, runtime, send_lines, start_topology, trace_init, wait_for_tcp,
        CountReceiver,
    },
    transforms,
};

#[test]
fn pipe() {
    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_sink(
        "out",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );

    let mut rt = runtime();
    rt.block_on_std(async move {
        let mut output_lines = CountReceiver::receive_lines(out_addr);

        let (topology, _crash) = start_topology(config, false).await;
        // Wait for server to accept traffic
        wait_for_tcp(in_addr).await;

        // Wait for output to connect
        output_lines.connected().await;

        let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
        send_lines(in_addr, input_lines.clone()).await.unwrap();

        // Shut down server
        topology.stop().compat().await.unwrap();

        let output_lines = output_lines.wait().await;
        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    });
}

#[test]
fn sample() {
    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_transform(
        "sampler",
        &["in"],
        transforms::sampler::SamplerConfig {
            rate: 10,
            key_field: None,
            pass_list: vec![],
        },
    );
    config.add_sink(
        "out",
        &["sampler"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );

    let mut rt = runtime();
    rt.block_on_std(async move {
        let mut output_lines = CountReceiver::receive_lines(out_addr);

        let (topology, _crash) = start_topology(config, false).await;
        // Wait for server to accept traffic
        wait_for_tcp(in_addr).await;

        // Wait for output to connect
        output_lines.connected().await;

        let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
        send_lines(in_addr, input_lines.clone()).await.unwrap();

        // Shut down server
        topology.stop().compat().await.unwrap();

        let output_lines = output_lines.wait().await;
        let num_output_lines = output_lines.len();

        let output_lines_ratio = num_output_lines as f32 / num_lines as f32;
        assert_relative_eq!(output_lines_ratio, 0.1, epsilon = 0.01);

        let mut input_lines = input_lines.into_iter();
        // Assert that all of the output lines were present in the input and in the same order
        for output_line in output_lines {
            let next_line = input_lines.by_ref().find(|l| l == &output_line);
            assert_eq!(Some(output_line), next_line);
        }
    });
}

#[test]
fn fork() {
    let num_lines: usize = 10000;

    let in_addr = next_addr();
    let out_addr1 = next_addr();
    let out_addr2 = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_sink(
        "out1",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr1.to_string()),
    );
    config.add_sink(
        "out2",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr2.to_string()),
    );

    let mut rt = runtime();
    rt.block_on_std(async move {
        let mut output_lines1 = CountReceiver::receive_lines(out_addr1);
        let mut output_lines2 = CountReceiver::receive_lines(out_addr2);

        let (topology, _crash) = start_topology(config, false).await;
        // Wait for server to accept traffic
        wait_for_tcp(in_addr).await;

        // Wait for output to connect
        output_lines1.connected().await;
        output_lines2.connected().await;

        let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
        send_lines(in_addr, input_lines.clone()).await.unwrap();

        // Shut down server
        topology.stop().compat().await.unwrap();

        let output_lines1 = output_lines1.wait().await;
        let output_lines2 = output_lines2.wait().await;
        assert_eq!(num_lines, output_lines1.len());
        assert_eq!(num_lines, output_lines2.len());
        assert_eq!(input_lines, output_lines1);
        assert_eq!(input_lines, output_lines2);
    });
}

#[test]
fn merge_and_fork() {
    trace_init();

    let num_lines: usize = 10000;

    let in_addr1 = next_addr();
    let in_addr2 = next_addr();
    let out_addr1 = next_addr();
    let out_addr2 = next_addr();

    // out1 receives both in1 and in2
    // out2 receives in2 only
    let mut config = config::Config::empty();
    config.add_source(
        "in1",
        sources::socket::SocketConfig::make_tcp_config(in_addr1),
    );
    config.add_source(
        "in2",
        sources::socket::SocketConfig::make_tcp_config(in_addr2),
    );
    config.add_sink(
        "out1",
        &["in1", "in2"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr1.to_string()),
    );
    config.add_sink(
        "out2",
        &["in2"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr2.to_string()),
    );

    let mut rt = Runtime::with_thread_count(2).unwrap();
    rt.block_on_std(async move {
        let mut output_lines1 = CountReceiver::receive_lines(out_addr1);
        let mut output_lines2 = CountReceiver::receive_lines(out_addr2);

        let (topology, _crash) = start_topology(config, false).await;
        // Wait for server to accept traffic
        wait_for_tcp(in_addr1).await;
        wait_for_tcp(in_addr2).await;

        // Wait for output to connect
        output_lines1.connected().await;
        output_lines2.connected().await;

        let input_lines1 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        let input_lines2 = random_lines(100).take(num_lines).collect::<Vec<_>>();
        send_lines(in_addr1, input_lines1.clone()).await.unwrap();
        send_lines(in_addr2, input_lines2.clone()).await.unwrap();

        // Shut down server
        topology.stop().compat().await.unwrap();

        let output_lines1 = output_lines1.wait().await;
        let output_lines2 = output_lines2.wait().await;

        assert_eq!(num_lines, output_lines2.len());

        assert_eq!(input_lines2, output_lines2);

        assert_eq!(num_lines * 2, output_lines1.len());
        // Assert that all of the output lines were present in the input and in the same order
        let mut input_lines1 = input_lines1.into_iter().peekable();
        let mut input_lines2 = input_lines2.into_iter().peekable();
        for output_line in &output_lines1 {
            if Some(output_line) == input_lines1.peek() {
                input_lines1.next();
            } else if Some(output_line) == input_lines2.peek() {
                input_lines2.next();
            } else {
                panic!("Got line in output that wasn't in input");
            }
        }
        assert_eq!(input_lines1.next(), None);
        assert_eq!(input_lines2.next(), None);
    });
}

#[test]
fn reconnect() {
    let num_lines: usize = 1000;

    let in_addr = next_addr();
    let out_addr = next_addr();

    let mut config = config::Config::empty();
    config.add_source(
        "in",
        sources::socket::SocketConfig::make_tcp_config(in_addr),
    );
    config.add_sink(
        "out",
        &["in"],
        sinks::socket::SocketSinkConfig::make_basic_tcp_config(out_addr.to_string()),
    );

    let mut rt = runtime();
    rt.block_on_std(async move {
        let output_lines = CountReceiver::receive_lines(out_addr);

        let (topology, _crash) = start_topology(config, false).await;
        // Wait for server to accept traffic
        wait_for_tcp(in_addr).await;

        let input_lines = random_lines(100).take(num_lines).collect::<Vec<_>>();
        send_lines(in_addr, input_lines.clone()).await.unwrap();

        // Shut down server and wait for it to fully flush
        topology.stop().compat().await.unwrap();

        let output_lines = output_lines.wait().await;
        assert!(num_lines >= 2);
        assert!(output_lines.iter().all(|line| input_lines.contains(line)))
    });
}

#[tokio::test]
async fn healthcheck() {
    let addr = next_addr();
    let resolver = vector::dns::Resolver;

    let _listener = TcpListener::bind(&addr).await.unwrap();

    let healthcheck = vector::sinks::util::tcp::tcp_healthcheck(
        addr.ip().to_string(),
        addr.port(),
        resolver,
        None.into(),
    );

    assert!(healthcheck.compat().await.is_ok());

    let bad_addr = next_addr();
    let bad_healthcheck = vector::sinks::util::tcp::tcp_healthcheck(
        bad_addr.ip().to_string(),
        bad_addr.port(),
        resolver,
        None.into(),
    );

    assert!(bad_healthcheck.compat().await.is_err());
}
