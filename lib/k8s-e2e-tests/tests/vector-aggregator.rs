use k8s_e2e_tests::*;
use k8s_test_framework::{lock, vector::Config as VectorConfig};

const HELM_CHART_VECTOR_AGGREGATOR: &str = "vector-aggregator";

const HELM_VALUES_DUMMY_TOPOLOGY: &str = r#"
sources:
  dummy:
    type: "generator"
    rawConfig: |
      lines = ["Hello world"]
      batch_interval = 60 # once a minute

sinks:
  stdout:
    type: "console"
    inputs: ["dummy"]
    rawConfig: |
      target = "stdout"
      encoding = "json"
"#;

/// This test validates that vector-aggregator can deploy with the default
/// settings and a dummy topology.
#[tokio::test]
async fn dummy_topology() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let framework = make_framework();

    let vector = framework
        .vector(
            "test-vector",
            HELM_CHART_VECTOR_AGGREGATOR,
            VectorConfig {
                custom_helm_values: HELM_VALUES_DUMMY_TOPOLOGY,
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            "test-vector",
            "statefulset/vector-aggregator",
            vec!["--timeout=60s"],
        )
        .await?;

    drop(vector);
    Ok(())
}
