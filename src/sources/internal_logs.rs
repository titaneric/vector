use crate::{
    config::{log_schema, DataType, SourceConfig, SourceContext, SourceDescription},
    event::Event,
    shutdown::ShutdownSignal,
    trace, Pipeline,
};
use futures::{stream, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InternalLogsConfig {
    host_key: Option<String>,
    pid_key: Option<String>,
}

inventory::submit! {
    SourceDescription::new::<InternalLogsConfig>("internal_logs")
}

impl_generate_config_from_default!(InternalLogsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "internal_logs")]
impl SourceConfig for InternalLogsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let host_key = self
            .host_key
            .as_deref()
            .unwrap_or_else(|| log_schema().host_key())
            .to_owned();
        let pid_key = self.pid_key.as_deref().unwrap_or("pid").to_owned();

        Ok(Box::pin(run(host_key, pid_key, cx.out, cx.shutdown)))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "internal_logs"
    }
}

async fn run(
    host_key: String,
    pid_key: String,
    out: Pipeline,
    mut shutdown: ShutdownSignal,
) -> Result<(), ()> {
    let mut out = out.sink_map_err(|error| error!(message = "Error sending log.", %error));
    let subscription = trace::subscribe();
    let mut rx = subscription.receiver;

    let hostname = crate::get_hostname();
    let pid = std::process::id();

    out.send_all(&mut stream::iter(subscription.buffer).map(|mut log| {
        if let Ok(hostname) = &hostname {
            log.insert(host_key.clone(), hostname.to_owned());
        }
        log.insert(pid_key.clone(), pid);
        Ok(Event::from(log))
    }))
    .await?;

    // Note: This loop, or anything called within it, MUST NOT generate
    // any logs that don't break the loop, as that could cause an
    // infinite loop since it receives all such logs.
    loop {
        tokio::select! {
            receive = rx.recv() => {
                match receive {
                    Ok(event) => out.send(Event::from(event)).await?,
                    Err(RecvError::Lagged(_)) => (),
                    Err(RecvError::Closed) => break,
                }
            }
            _ = &mut shutdown => break,
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event::Event, test_util::collect_ready, trace};
    use futures::channel::mpsc;
    use tokio::time::{sleep, Duration};
    use vector_core::event::Value;

    #[test]
    fn generates_config() {
        crate::test_util::test_generate_config::<InternalLogsConfig>();
    }

    #[tokio::test]
    async fn receives_logs() {
        let test_id: u8 = rand::random();
        let start = chrono::Utc::now();
        trace::init(false, false, "debug");
        trace::reset_early_buffer();
        error!(message = "Before source started.", %test_id);

        let rx = start_source().await;

        error!(message = "After source started.", %test_id);

        sleep(Duration::from_millis(1)).await;
        let mut events = collect_ready(rx).await;
        let test_id = Value::from(test_id.to_string());
        events.retain(|event| event.as_log().get("test_id") == Some(&test_id));

        let end = chrono::Utc::now();

        assert_eq!(events.len(), 2);

        assert_eq!(
            events[0].as_log()["message"],
            "Before source started.".into()
        );
        assert_eq!(
            events[1].as_log()["message"],
            "After source started.".into()
        );

        for event in events {
            let log = event.as_log();
            let timestamp = *log["timestamp"]
                .as_timestamp()
                .expect("timestamp isn't a timestamp");
            assert!(timestamp >= start);
            assert!(timestamp <= end);
            assert_eq!(log["metadata.kind"], "event".into());
            assert_eq!(log["metadata.level"], "ERROR".into());
        }
    }

    async fn start_source() -> mpsc::Receiver<Event> {
        let (tx, rx) = Pipeline::new_test();

        let source = InternalLogsConfig::default()
            .build(SourceContext::new_test(tx))
            .await
            .unwrap();
        tokio::spawn(source);
        sleep(Duration::from_millis(1)).await;
        trace::stop_buffering();
        rx
    }
}
