use futures::{SinkExt, StreamExt};
use k8s_e2e_tests::*;
use k8s_test_framework::{
    lock, test_pod, vector::Config as VectorConfig, wait_for_resource::WaitFor,
};
use std::collections::HashSet;

const HELM_CHART_VECTOR_AGENT: &str = "vector-agent";

const HELM_VALUES_STDOUT_SINK: &str = r#"
sinks:
  stdout:
    type: "console"
    inputs: ["kubernetes_logs"]
    rawConfig: |
      target = "stdout"
      encoding = "json"
"#;

const CUSTOM_RESOURCE_VECTOR_CONFIG: &str = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: vector-agent-config
data:
  vector.toml: |
    [sinks.stdout]
        type = "console"
        inputs = ["kubernetes_logs"]
        target = "stdout"
        encoding = "json"
"#;

/// This test validates that vector-agent picks up logs at the simplest case
/// possible - a new pod is deployed and prints to stdout, and we assert that
/// vector picks that up.
#[tokio::test]
async fn simple() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let framework = make_framework();

    let vector = framework
        .vector(
            "test-vector",
            HELM_CHART_VECTOR_AGENT,
            VectorConfig {
                custom_helm_values: HELM_VALUES_STDOUT_SINK,
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            "test-vector",
            "daemonset/vector-agent",
            vec!["--timeout=60s"],
        )
        .await?;

    let test_namespace = framework.namespace("test-vector-test-pod").await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            "test-vector-test-pod",
            "test-pod",
            "echo MARKER",
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            "test-vector-test-pod",
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut log_reader = framework.logs("test-vector", "daemonset/vector-agent")?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_marker = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != "test-vector-test-pod" {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");

        if got_marker {
            // We've already seen one marker! This is not good, we only emitted
            // one.
            panic!("Marker seen more than once");
        }

        // If we did, remember it.
        got_marker = true;

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    assert!(got_marker);

    drop(test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}

/// This test validates that vector-agent properly merges a log message that
/// kubernetes has internally split into multiple partial log lines.
#[tokio::test]
async fn partial_merge() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let framework = make_framework();

    let vector = framework
        .vector(
            "test-vector",
            HELM_CHART_VECTOR_AGENT,
            VectorConfig {
                custom_helm_values: HELM_VALUES_STDOUT_SINK,
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            "test-vector",
            "daemonset/vector-agent",
            vec!["--timeout=60s"],
        )
        .await?;

    let test_namespace = framework.namespace("test-vector-test-pod").await?;

    let test_message = generate_long_string(8, 8 * 1024); // 64 KiB
    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            "test-vector-test-pod",
            "test-pod",
            &format!("echo {}", test_message),
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            "test-vector-test-pod",
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut log_reader = framework.logs("test-vector", "daemonset/vector-agent")?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_expected_line = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != "test-vector-test-pod" {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Ensure the message we got matches the one we emitted.
        assert_eq!(val["message"], test_message);

        if got_expected_line {
            // We've already seen our expected line once! This is not good, we
            // only emitted one.
            panic!("Test message seen more than once");
        }

        // If we did, remember it.
        got_expected_line = true;

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    assert!(got_expected_line);

    drop(test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}

/// This test validates that vector-agent picks up preexisting logs - logs that
/// existed before vector was deployed.
#[tokio::test]
async fn preexisting() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let framework = make_framework();

    let test_namespace = framework.namespace("test-vector-test-pod").await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            "test-vector-test-pod",
            "test-pod",
            "echo MARKER",
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            "test-vector-test-pod",
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Wait for some extra time to ensure pod completes.
    tokio::time::delay_for(std::time::Duration::from_secs(10)).await;

    let vector = framework
        .vector(
            "test-vector",
            HELM_CHART_VECTOR_AGENT,
            VectorConfig {
                custom_helm_values: HELM_VALUES_STDOUT_SINK,
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            "test-vector",
            "daemonset/vector-agent",
            vec!["--timeout=60s"],
        )
        .await?;

    let mut log_reader = framework.logs("test-vector", "daemonset/vector-agent")?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_marker = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != "test-vector-test-pod" {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");

        if got_marker {
            // We've already seen one marker! This is not good, we only emitted
            // one.
            panic!("Marker seen more than once");
        }

        // If we did, remember it.
        got_marker = true;

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    assert!(got_marker);

    drop(test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}

/// This test validates that vector-agent picks up multiple log lines, and that
/// they arrive at the proper order.
#[tokio::test]
async fn multiple_lines() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let framework = make_framework();

    let vector = framework
        .vector(
            "test-vector",
            HELM_CHART_VECTOR_AGENT,
            VectorConfig {
                custom_helm_values: HELM_VALUES_STDOUT_SINK,
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            "test-vector",
            "daemonset/vector-agent",
            vec!["--timeout=60s"],
        )
        .await?;

    let test_namespace = framework.namespace("test-vector-test-pod").await?;

    let test_messages = vec!["MARKER1", "MARKER2", "MARKER3", "MARKER4", "MARKER5"];
    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            "test-vector-test-pod",
            "test-pod",
            &format!("echo -e {}", test_messages.join(r"\\n")),
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            "test-vector-test-pod",
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut log_reader = framework.logs("test-vector", "daemonset/vector-agent")?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut test_messages_iter = test_messages.into_iter().peekable();
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != "test-vector-test-pod" {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Take the next marker.
        let current_marker = test_messages_iter
            .next()
            .expect("expected no more lines since the test messages iter is exhausted");

        // Ensure we got the marker.
        assert_eq!(val["message"], current_marker);

        if test_messages_iter.peek().is_some() {
            // We're not done yet, so go on.
            return FlowControlCommand::GoOn;
        }

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    assert!(test_messages_iter.next().is_none());

    drop(test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}

/// This test validates that vector-agent properly annotates log events with pod
/// metadata obtained from the k8s API.
#[tokio::test]
async fn pod_metadata_annotation() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let framework = make_framework();

    let vector = framework
        .vector(
            "test-vector",
            HELM_CHART_VECTOR_AGENT,
            VectorConfig {
                custom_helm_values: HELM_VALUES_STDOUT_SINK,
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            "test-vector",
            "daemonset/vector-agent",
            vec!["--timeout=60s"],
        )
        .await?;

    let test_namespace = framework.namespace("test-vector-test-pod").await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            "test-vector-test-pod",
            "test-pod",
            "echo MARKER",
            vec![("label1", "hello"), ("label2", "world")],
        ))?)
        .await?;
    framework
        .wait(
            "test-vector-test-pod",
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut log_reader = framework.logs("test-vector", "daemonset/vector-agent")?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_marker = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != "test-vector-test-pod" {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");

        if got_marker {
            // We've already seen one marker! This is not good, we only emitted
            // one.
            panic!("Marker seen more than once");
        }

        // If we did, remember it.
        got_marker = true;

        // Assert pod the event is properly annotated with pod metadata.
        assert_eq!(val["kubernetes"]["pod_name"], "test-pod");
        // We've already asserted this above, but repeat for completeness.
        assert_eq!(val["kubernetes"]["pod_namespace"], "test-vector-test-pod");
        assert_eq!(val["kubernetes"]["pod_uid"].as_str().unwrap().len(), 36); // 36 is a standard UUID string length
        assert_eq!(val["kubernetes"]["pod_labels"]["label1"], "hello");
        assert_eq!(val["kubernetes"]["pod_labels"]["label2"], "world");
        // We don't have the node name to compare this to, so just assert it's
        // a non-empty string.
        assert!(!val["kubernetes"]["pod_node_name"]
            .as_str()
            .unwrap()
            .is_empty());
        assert_eq!(val["kubernetes"]["container_name"], "test-pod");
        assert_eq!(val["kubernetes"]["container_image"], BUSYBOX_IMAGE);

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    assert!(got_marker);

    drop(test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}

/// This test validates that vector-agent properly filters out the logs that are
/// requested to be excluded from collection, based on k8s API `Pod` labels.
#[tokio::test]
async fn pod_filtering() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let framework = make_framework();

    let vector = framework
        .vector(
            "test-vector",
            HELM_CHART_VECTOR_AGENT,
            VectorConfig {
                custom_helm_values: HELM_VALUES_STDOUT_SINK,
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            "test-vector",
            "daemonset/vector-agent",
            vec!["--timeout=60s"],
        )
        .await?;

    let test_namespace = framework.namespace("test-vector-test-pod").await?;

    let excluded_test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            "test-vector-test-pod",
            "test-pod-excluded",
            "echo EXCLUDED_MARKER",
            vec![("vector.dev/exclude", "true")],
        ))?)
        .await?;
    framework
        .wait(
            "test-vector-test-pod",
            vec!["pods/test-pod-excluded"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    let control_test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            "test-vector-test-pod",
            "test-pod-control",
            "echo CONTROL_MARKER",
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            "test-vector-test-pod",
            vec!["pods/test-pod-control"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut log_reader = framework.logs("test-vector", "daemonset/vector-agent")?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the log lines until the reasonable amount of time passes for us
    // to be confident that vector should've picked up the excluded message
    // if it wasn't filtering it.
    let mut got_control_marker = false;
    let mut lines_till_we_give_up: usize = 10000;
    let (stop_tx, mut stop_rx) = futures::channel::mpsc::channel(0);
    loop {
        let line = tokio::select! {
            result = stop_rx.next() => {
                result.unwrap();
                log_reader.kill()?;
                continue;
            }
            line = log_reader.read_line() => line,
        };
        let line = match line {
            Some(line) => line,
            None => break,
        };
        println!("Got line: {:?}", line);

        lines_till_we_give_up -= 1;
        if lines_till_we_give_up <= 0 {
            println!("Giving up");
            log_reader.kill()?;
            break;
        }

        if !line.starts_with("{") {
            // This isn't a json, must be an entry from Vector's own log stream.
            continue;
        }

        let val = parse_json(&line)?;

        if val["kubernetes"]["pod_namespace"] != "test-vector-test-pod" {
            // A log from something other than our test pod, pretend we don't
            // see it.
            continue;
        }

        // Ensure we got the log event from the control pod.
        assert_eq!(val["kubernetes"]["pod_name"], "test-pod-control");

        // Ensure the test sanity by validating that we got the control marker.
        // If we get an excluded marker here - it's an error.
        assert_eq!(val["message"], "CONTROL_MARKER");

        if got_control_marker {
            // We've already seen one control marker! This is not good, we only
            // emitted one.
            panic!("Control marker seen more than once");
        }

        // Remember that we've seen a control marker.
        got_control_marker = true;

        // Request termination in a while.
        let mut stop_tx = stop_tx.clone();
        tokio::spawn(async move {
            // Wait for two minutes - a reasonable time for vector internals to
            // pick up new `Pod` and collect events from them in idle load.
            // Here, we're assuming that if the `Pod` that was supposed to be
            // ignored was in fact collected (meaning something's wrong with
            // the exclusion logic), we'd see it's data within this time frame.
            // It's not enough to just wait for `Pod` complete, we should still
            // apply a reasonably big timeout before we stop waiting for the
            // logs to appear to have high confidence that Vector has enough
            // time to pick them up and spit them out.
            let duration = std::time::Duration::from_secs(120);
            println!("Starting stop timer, due in {} seconds", duration.as_secs());
            tokio::time::delay_for(duration).await;
            println!("Stop timer complete");
            stop_tx.send(()).await.unwrap();
        });
    }

    // Ensure log reader exited.
    log_reader.wait().await.expect("log reader wait failed");

    assert!(got_control_marker);

    drop(excluded_test_pod);
    drop(control_test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}

/// This test validates that vector-agent properly filters out the logs by the
/// custom selectors, based on k8s API `Pod` labels and annotations.
#[tokio::test]
async fn custom_selectors() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let framework = make_framework();

    const CONFIG: &str = r#"
kubernetesLogsSource:
  rawConfig: |
    extra_label_selector = "my_custom_negative_label_selector!=my_val"
    extra_field_selector = "metadata.name!=test-pod-excluded-by-name"
"#;

    let vector = framework
        .vector(
            "test-vector",
            HELM_CHART_VECTOR_AGENT,
            VectorConfig {
                custom_helm_values: &format!("{}\n{}", CONFIG, HELM_VALUES_STDOUT_SINK),
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            "test-vector",
            "daemonset/vector-agent",
            vec!["--timeout=60s"],
        )
        .await?;

    let test_namespace = framework.namespace("test-vector-test-pod").await?;

    let label_sets = vec![
        ("test-pod-excluded-1", vec![("vector.dev/exclude", "true")]),
        (
            "test-pod-excluded-2",
            vec![("my_custom_negative_label_selector", "my_val")],
        ),
        ("test-pod-excluded-by-name", vec![]),
    ];
    let mut excluded_test_pods = Vec::new();
    let mut excluded_test_pod_names = Vec::new();
    for (name, label_set) in label_sets {
        excluded_test_pods.push(
            framework
                .test_pod(test_pod::Config::from_pod(&make_test_pod(
                    "test-vector-test-pod",
                    name,
                    "echo EXCLUDED_MARKER",
                    label_set,
                ))?)
                .await?,
        );
        excluded_test_pod_names.push(name);
    }
    for name in excluded_test_pod_names {
        let name = format!("pods/{}", name);
        framework
            .wait(
                "test-vector-test-pod",
                vec![name.as_ref()],
                WaitFor::Condition("initialized"),
                vec!["--timeout=60s"],
            )
            .await?;
    }

    let control_test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            "test-vector-test-pod",
            "test-pod-control",
            "echo CONTROL_MARKER",
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            "test-vector-test-pod",
            vec!["pods/test-pod-control"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut log_reader = framework.logs("test-vector", "daemonset/vector-agent")?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the log lines until the reasonable amount of time passes for us
    // to be confident that vector should've picked up the excluded message
    // if it wasn't filtering it.
    let mut got_control_marker = false;
    let mut lines_till_we_give_up: usize = 10000;
    let (stop_tx, mut stop_rx) = futures::channel::mpsc::channel(0);
    loop {
        let line = tokio::select! {
            result = stop_rx.next() => {
                result.unwrap();
                log_reader.kill()?;
                continue;
            }
            line = log_reader.read_line() => line,
        };
        let line = match line {
            Some(line) => line,
            None => break,
        };
        println!("Got line: {:?}", line);

        lines_till_we_give_up -= 1;
        if lines_till_we_give_up <= 0 {
            println!("Giving up");
            log_reader.kill()?;
            break;
        }

        if !line.starts_with("{") {
            // This isn't a json, must be an entry from Vector's own log stream.
            continue;
        }

        let val = parse_json(&line)?;

        if val["kubernetes"]["pod_namespace"] != "test-vector-test-pod" {
            // A log from something other than our test pod, pretend we don't
            // see it.
            continue;
        }

        // Ensure we got the log event from the control pod.
        assert_eq!(val["kubernetes"]["pod_name"], "test-pod-control");

        // Ensure the test sanity by validating that we got the control marker.
        // If we get an excluded marker here - it's an error.
        assert_eq!(val["message"], "CONTROL_MARKER");

        if got_control_marker {
            // We've already seen one control marker! This is not good, we only
            // emitted one.
            panic!("Control marker seen more than once");
        }

        // Remember that we've seen a control marker.
        got_control_marker = true;

        // Request termination in a while.
        let mut stop_tx = stop_tx.clone();
        tokio::spawn(async move {
            // Wait for two minutes - a reasonable time for vector internals to
            // pick up new `Pod` and collect events from them in idle load.
            // Here, we're assuming that if the `Pod` that was supposed to be
            // ignored was in fact collected (meaning something's wrong with
            // the exclusion logic), we'd see it's data within this time frame.
            // It's not enough to just wait for `Pod` complete, we should still
            // apply a reasonably big timeout before we stop waiting for the
            // logs to appear to have high confidence that Vector has enough
            // time to pick them up and spit them out.
            let duration = std::time::Duration::from_secs(120);
            println!("Starting stop timer, due in {} seconds", duration.as_secs());
            tokio::time::delay_for(duration).await;
            println!("Stop timer complete");
            stop_tx.send(()).await.unwrap();
        });
    }

    // Ensure log reader exited.
    log_reader.wait().await.expect("log reader wait failed");

    assert!(got_control_marker);

    drop(excluded_test_pods);
    drop(control_test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}

/// This test validates that vector-agent properly collects logs from multiple
/// `Namespace`s and `Pod`s.
#[tokio::test]
async fn multiple_ns() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let framework = make_framework();

    let vector = framework
        .vector(
            "test-vector",
            HELM_CHART_VECTOR_AGENT,
            VectorConfig {
                custom_helm_values: HELM_VALUES_STDOUT_SINK,
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            "test-vector",
            "daemonset/vector-agent",
            vec!["--timeout=60s"],
        )
        .await?;

    const NS_PREFIX: &str = "test-vector-test-pod";

    let mut test_namespaces = vec![];
    let mut expected_namespaces = HashSet::new();
    for i in 0..10 {
        let name = format!("{}-{}", NS_PREFIX, i);
        test_namespaces.push(framework.namespace(&name).await?);
        expected_namespaces.insert(name);
    }

    let mut test_pods = vec![];
    for ns in &expected_namespaces {
        let test_pod = framework
            .test_pod(test_pod::Config::from_pod(&make_test_pod(
                ns,
                "test-pod",
                "echo MARKER",
                vec![],
            ))?)
            .await?;
        framework
            .wait(
                ns,
                vec!["pods/test-pod"],
                WaitFor::Condition("initialized"),
                vec!["--timeout=60s"],
            )
            .await?;
        test_pods.push(test_pod);
    }

    let mut log_reader = framework.logs("test-vector", "daemonset/vector-agent")?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    look_for_log_line(&mut log_reader, |val| {
        let ns = match val["kubernetes"]["pod_namespace"].as_str() {
            Some(val) if val.starts_with(NS_PREFIX) => val,
            _ => {
                // A log from something other than our test pod, pretend we
                // don't see it.
                return FlowControlCommand::GoOn;
            }
        };

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");

        // Remove the namespace from the list of namespaces we still expect to
        // get.
        let as_expected = expected_namespaces.remove(ns);
        assert!(as_expected);

        if expected_namespaces.is_empty() {
            // We got all the messages we expected, request to stop the flow.
            FlowControlCommand::Terminate
        } else {
            // We didn't get all the messages yet.
            FlowControlCommand::GoOn
        }
    })
    .await?;

    // Ensure that we have collected messages from all the namespaces.
    assert!(expected_namespaces.is_empty());

    drop(test_pods);
    drop(test_namespaces);
    drop(vector);
    Ok(())
}

/// This test validates that vector-agent helm chart properly allows
/// configuration via an additional config file, i.e. it can combine the managed
/// and custom config files.
#[tokio::test]
async fn additional_config_file() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let framework = make_framework();

    let vector = framework
        .vector(
            "test-vector",
            HELM_CHART_VECTOR_AGENT,
            VectorConfig {
                custom_resource: CUSTOM_RESOURCE_VECTOR_CONFIG,
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            "test-vector",
            "daemonset/vector-agent",
            vec!["--timeout=60s"],
        )
        .await?;

    let test_namespace = framework.namespace("test-vector-test-pod").await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            "test-vector-test-pod",
            "test-pod",
            "echo MARKER",
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            "test-vector-test-pod",
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut log_reader = framework.logs("test-vector", "daemonset/vector-agent")?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_marker = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != "test-vector-test-pod" {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");

        if got_marker {
            // We've already seen one marker! This is not good, we only emitted
            // one.
            panic!("Marker seen more than once");
        }

        // If we did, remember it.
        got_marker = true;

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    assert!(got_marker);

    drop(test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}
