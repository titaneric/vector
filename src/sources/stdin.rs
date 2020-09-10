use crate::{
    config::{log_schema, DataType, GlobalOptions, SourceConfig, SourceDescription},
    event::Event,
    internal_events::{StdinEventReceived, StdinReadFailed},
    shutdown::ShutdownSignal,
    Pipeline,
};
use bytes::Bytes;
use futures::{
    compat::{Future01CompatExt, Sink01CompatExt},
    executor, FutureExt, StreamExt, TryFutureExt, TryStreamExt,
};
use futures01::Sink;
use serde::{Deserialize, Serialize};
use std::{io, thread};
use tokio::sync::mpsc::channel;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct StdinConfig {
    #[serde(default = "default_max_length")]
    pub max_length: usize,
    pub host_key: Option<String>,
}

impl Default for StdinConfig {
    fn default() -> Self {
        StdinConfig {
            max_length: default_max_length(),
            host_key: None,
        }
    }
}

fn default_max_length() -> usize {
    bytesize::kib(100u64) as usize
}

inventory::submit! {
    SourceDescription::new::<StdinConfig>("stdin")
}

#[typetag::serde(name = "stdin")]
impl SourceConfig for StdinConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        stdin_source(io::BufReader::new(io::stdin()), self.clone(), shutdown, out)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "stdin"
    }
}

pub fn stdin_source<R>(
    stdin: R,
    config: StdinConfig,
    shutdown: ShutdownSignal,
    out: Pipeline,
) -> crate::Result<super::Source>
where
    R: Send + io::BufRead + 'static,
{
    let host_key = config
        .host_key
        .unwrap_or_else(|| log_schema().host_key().to_string());
    let hostname = hostname::get_hostname();

    let (mut sender, receiver) = channel(1024);

    // Start the background thread
    thread::spawn(move || {
        info!("Capturing STDIN.");

        for line in stdin.lines() {
            if executor::block_on(sender.send(line)).is_err() {
                // receiver has closed so we should shutdown
                return;
            }
        }
    });

    let fut = receiver
        .take_until(shutdown.compat())
        .map_err(|error| emit!(StdinReadFailed { error }))
        .map_ok(move |line| {
            emit!(StdinEventReceived {
                byte_size: line.len()
            });
            create_event(Bytes::from(line), &host_key, &hostname)
        })
        .forward(
            out.sink_map_err(|error| error!(message = "Unable to send event to out.", %error))
                .sink_compat(),
        )
        .inspect(|_| info!("Finished sending"));

    Ok(Box::new(fut.boxed().compat()))
}

fn create_event(line: Bytes, host_key: &str, hostname: &Option<String>) -> Event {
    let mut event = Event::from(line);

    // Add source type
    event
        .as_mut_log()
        .insert(log_schema().source_type_key(), Bytes::from("stdin"));

    if let Some(hostname) = &hostname {
        event.as_mut_log().insert(host_key, hostname.clone());
    }

    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_util::trace_init, Pipeline};
    use futures::compat::Future01CompatExt;
    use futures01::{Async::*, Stream};
    use std::io::Cursor;

    #[test]
    fn stdin_create_event() {
        let line = Bytes::from("hello world");
        let host_key = "host".to_string();
        let hostname = Some("Some.Machine".to_string());

        let event = create_event(line, &host_key, &hostname);
        let log = event.into_log();

        assert_eq!(log[&"host".into()], "Some.Machine".into());
        assert_eq!(log[&log_schema().message_key()], "hello world".into());
        assert_eq!(log[log_schema().source_type_key()], "stdin".into());
    }

    #[tokio::test]
    async fn stdin_decodes_line() {
        trace_init();

        let (tx, mut rx) = Pipeline::new_test();
        let config = StdinConfig::default();
        let buf = Cursor::new("hello world\nhello world again");

        stdin_source(buf, config, ShutdownSignal::noop(), tx)
            .unwrap()
            .compat()
            .await
            .unwrap();

        let event = rx.poll().unwrap();

        assert!(event.is_ready());
        assert_eq!(
            Ready(Some("hello world".into())),
            event.map(|event| event
                .map(|event| event.as_log()[&log_schema().message_key()].to_string_lossy()))
        );

        let event = rx.poll().unwrap();
        assert!(event.is_ready());
        assert_eq!(
            Ready(Some("hello world again".into())),
            event.map(|event| event
                .map(|event| event.as_log()[&log_schema().message_key()].to_string_lossy()))
        );

        let event = rx.poll().unwrap();
        assert!(event.is_ready());
        assert_eq!(Ready(None), event);
    }
}
