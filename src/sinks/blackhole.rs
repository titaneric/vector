use crate::{
    buffers::Acker,
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    emit,
    event::Event,
    internal_events::BlackholeEventReceived,
};
use futures::{future, FutureExt};
use futures01::{AsyncSink, Poll, Sink, StartSend};
use serde::{Deserialize, Serialize};

pub struct BlackholeSink {
    total_events: usize,
    total_raw_bytes: usize,
    config: BlackholeConfig,
    acker: Acker,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BlackholeConfig {
    pub print_amount: usize,
}

inventory::submit! {
    SinkDescription::new_without_default::<BlackholeConfig>("blackhole")
}

#[typetag::serde(name = "blackhole")]
impl SinkConfig for BlackholeConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink = Box::new(BlackholeSink::new(self.clone(), cx.acker()));
        let healthcheck = future::ok(()).boxed();

        Ok((super::VectorSink::Futures01Sink(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "blackhole"
    }
}

impl BlackholeSink {
    pub fn new(config: BlackholeConfig, acker: Acker) -> Self {
        BlackholeSink {
            config,
            total_events: 0,
            total_raw_bytes: 0,
            acker,
        }
    }
}

impl Sink for BlackholeSink {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let message_len = match item {
            Event::Log(log) => log
                .get(&crate::config::log_schema().message_key())
                .map(|v| v.as_bytes().len())
                .unwrap_or(0),
            Event::Metric(metric) => serde_json::to_string(&metric).map(|v| v.len()).unwrap_or(0),
        };

        self.total_events += 1;
        self.total_raw_bytes += message_len;

        emit!(BlackholeEventReceived {
            byte_size: message_len
        });

        if self.total_events % self.config.print_amount == 0 {
            info!({
                events = self.total_events,
                raw_bytes_collected = self.total_raw_bytes
            }, "Total events collected");
        }

        self.acker.ack(1);

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{buffers::Acker, test_util::random_events_with_stream};
    use futures::{compat::Sink01CompatExt, SinkExt};

    #[tokio::test]
    async fn blackhole() {
        let config = BlackholeConfig { print_amount: 10 };
        let sink = BlackholeSink::new(config, Acker::Null);

        let (_input_lines, mut events) = random_events_with_stream(100, 10);

        let _ = sink.sink_compat().send_all(&mut events).await.unwrap();
    }
}
