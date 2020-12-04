/// This helper function issues an HTTP request to the Prometheus-exposition
/// format metrics endpoint, validates that it completes successfully and
/// returns the response body.
pub async fn load(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let response = reqwest::get(url).await?.error_for_status()?;
    let body = response.text().await?;
    Ok(body)
}

fn metrics_regex() -> regex::Regex {
    regex::RegexBuilder::new(
        r"^(?P<name>[a-zA-Z_:][a-zA-Z0-9_:]*)(?P<labels>\{[^}]*\})? (?P<value>.+)$",
    )
    .multi_line(true)
    .build()
    .expect("invalid regex")
}

/// This helper function extracts the sum of `processed_events`-ish metrics
/// across all labels.
pub fn extract_processed_events_sum(metrics: &str) -> Result<u64, Box<dyn std::error::Error>> {
    metrics_regex()
        .captures_iter(&metrics)
        .filter_map(|captures| {
            let metric_name = &captures["name"];
            let value = &captures["value"];
            if !metric_name.contains("processed_events") {
                return None;
            }
            Some(value.to_owned())
        })
        .try_fold::<u64, _, Result<u64, Box<dyn std::error::Error>>>(0u64, |acc, value| {
            let value = value.parse::<u64>()?;
            let next_acc = acc.checked_add(value).ok_or("u64 overflow")?;
            Ok(next_acc)
        })
}

/// This helper function validates the presence of `vector_started`-ish metric.
pub fn extract_vector_started(metrics: &str) -> bool {
    metrics_regex().captures_iter(&metrics).any(|captures| {
        let metric_name = &captures["name"];
        let value = &captures["value"];
        metric_name.contains("vector_started") && value == "1"
    })
}

/// This helper function performs an HTTP request to the specified URL and
/// extracts the sum of `processed_events`-ish metrics across all labels.
pub async fn get_processed_events(url: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let metrics = load(url).await?;
    extract_processed_events_sum(&metrics)
}

/// This helper function performs an HTTP request to the specified URL and
/// validates the presence of `vector_started`-ish metric.
pub async fn assert_vector_started(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let metrics = load(url).await?;
    if !extract_vector_started(&metrics) {
        return Err(format!("vector_started metric was not found:\n{}", metrics).into());
    }
    Ok(())
}

/// This helper function performs HTTP requests to the specified URL and
/// waits for the presence of `vector_started`-ish metric until the deadline
/// with even dealys between attempts.
pub async fn wait_for_vector_started(
    url: &str,
    next_attempt_delay: std::time::Duration,
    deadline: std::time::Instant,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let err = match assert_vector_started(url).await {
            Ok(()) => break,
            Err(err) => err,
        };
        if std::time::Instant::now() >= deadline {
            return Err(err);
        }

        eprintln!(
            "Waiting for vector_started metrics to be available, next poll in {} sec, deadline at {:?}",
            next_attempt_delay.as_secs_f64(), deadline,
        );
        tokio::time::delay_for(next_attempt_delay).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_processed_events_sum() {
        let cases = vec![
            (vec![r#""#], 0),
            (vec![r#"processed_events 123"#], 123),
            (vec![r#"processed_events{} 123"#], 123),
            (vec![r#"processed_events{method="POST"} 456"#], 456),
            (vec![r#"processed_events{a="b",c="d"} 456"#], 456),
            (
                vec![
                    r#"processed_events 123"#,
                    r#"processed_events{method="POST"} 456"#,
                ],
                123 + 456,
            ),
            (vec![r#"other{} 789"#], 0),
            (
                vec![
                    r#"processed_events{} 123"#,
                    r#"processed_events{method="POST"} 456"#,
                    r#"other{} 789"#,
                ],
                123 + 456,
            ),
            // Prefixes and suffixes
            (
                vec![
                    r#"processed_events 1"#,
                    r#"processed_events_total 2"#,
                    r#"vector_processed_events 3"#,
                    r#"vector_processed_events_total 4"#,
                ],
                1 + 2 + 3 + 4,
            ),
        ];

        for (input, expected_value) in cases {
            let input = input.join("\n");
            let actual_value = extract_processed_events_sum(&input).unwrap();
            assert_eq!(expected_value, actual_value);
        }
    }

    #[test]
    fn test_extract_vector_started() {
        let cases = vec![
            (vec![r#"vector_started 1"#], true),
            (vec![r#"vector_started_total 1"#], true),
            (vec![r#"vector_vector_started_total 1"#], true),
            (vec![r#""#], false),
            (vec![r#"other{} 1"#], false),
            // Real-world example.
            (
                vec![
                    r#"# HELP vector_started_total vector_started_total"#,
                    r#"# TYPE vector_started_total counter"#,
                    r#"vector_started_total 1"#,
                ],
                true,
            ),
        ];

        for (input, expected_value) in cases {
            let input = input.join("\n");
            let actual_value = extract_vector_started(&input);
            assert_eq!(expected_value, actual_value, "input: {}", input);
        }
    }
}
