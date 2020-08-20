#[macro_use]
extern crate tracing;
#[macro_use]
extern crate vector;

mod cli;

use cli::{Color, LogFormat, Opts, SubCommand};
use futures::{
    compat::{Future01CompatExt, Stream01CompatExt},
    StreamExt,
};
use futures01::Future;
use std::cmp::max;
use tokio::select;
use vector::{
    config::{self, Config, ConfigDiff},
    event, generate, heartbeat,
    internal_events::{VectorReloaded, VectorStarted, VectorStopped},
    list, metrics, runtime,
    signal::{self, SignalTo},
    topology, trace, unit_test, validate,
};

fn main() {
    openssl_probe::init_ssl_cert_env_vars();

    let root_opts = Opts::get_matches();
    let level = root_opts.log_level();

    let opts = root_opts.root;
    let sub_command = root_opts.sub_command;

    let levels = match std::env::var("LOG").ok() {
        Some(level) => level,
        None => match level {
            "off" => "off".to_string(),
            _ => [
                format!("vector={}", level),
                format!("codec={}", level),
                format!("file_source={}", level),
                "tower_limit=trace".to_owned(),
                format!("rdkafka={}", level),
            ]
            .join(","),
        },
    };

    let color = match opts.color {
        #[cfg(unix)]
        Color::Auto => atty::is(atty::Stream::Stdout),
        #[cfg(windows)]
        Color::Auto => false, // ANSI colors are not supported by cmd.exe
        Color::Always => true,
        Color::Never => false,
    };

    let json = match &opts.log_format {
        LogFormat::Text => false,
        LogFormat::Json => true,
    };

    trace::init(color, json, levels.as_str());

    metrics::init().expect("metrics initialization failed");

    info!("Log level {:?} is enabled.", level);

    if let Some(threads) = opts.threads {
        if threads < 1 {
            error!("The `threads` argument must be greater or equal to 1.");
            std::process::exit(exitcode::CONFIG);
        }
    }

    let mut rt = {
        let threads = opts.threads.unwrap_or_else(|| max(1, num_cpus::get()));
        runtime::Runtime::with_thread_count(threads).expect("Unable to create async runtime")
    };

    rt.block_on_std(async move {
        if let Some(s) = sub_command {
            std::process::exit(match s {
                SubCommand::Validate(v) => validate::validate(&v, color).await,
                SubCommand::List(l) => list::cmd(&l),
                SubCommand::Test(t) => unit_test::cmd(&t),
                SubCommand::Generate(g) => generate::cmd(&g),
            })
        };

        let config_paths = config::process_paths(&opts.config_paths).unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });

        if opts.watch_config {
            // Start listening for config changes immediately.
            config::watcher::spawn_thread(&config_paths, None).unwrap_or_else(|error| {
                error!(message = "Unable to start config watcher.", %error);
                std::process::exit(exitcode::CONFIG);
            });
        }

        info!(
            message = "Loading configs.",
            path = ?config_paths
        );

        let read_config = config::read_configs(&config_paths);
        let maybe_config = handle_config_errors(read_config);
        let config = maybe_config.unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });
        event::LOG_SCHEMA
            .set(config.global.log_schema.clone())
            .expect("Couldn't set schema");

        let diff = ConfigDiff::initial(&config);
        let pieces = topology::validate(&config, &diff).await.unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });

        let result = topology::start_validated(config, diff, pieces, opts.require_healthy).await;
        let (mut topology, graceful_crash) = result.unwrap_or_else(|| {
            std::process::exit(exitcode::CONFIG);
        });

        emit!(VectorStarted);
        tokio::spawn(heartbeat::heartbeat());

        let mut signals = signal::signals();
        let mut sources_finished = topology.sources_finished().compat();
        let mut graceful_crash = graceful_crash.compat();

        let signal = loop {
            select! {
                Some(signal) = signals.next() => {
                    if signal == SignalTo::Reload {
                        // Reload config
                        let new_config = config::read_configs(&config_paths);

                        trace!("Parsing config");
                        let new_config = handle_config_errors(new_config);
                        if let Some(new_config) = new_config {
                            match topology
                                .reload_config_and_respawn(new_config, opts.require_healthy)
                                .await
                            {
                                Ok(true) =>  emit!(VectorReloaded { config_paths: &config_paths }),
                                Ok(false) => error!("Reload was not successful."),
                                // Trigger graceful shutdown for what remains of the topology
                                Err(()) => break SignalTo::Shutdown,
                            }
                        } else {
                            error!("Reload aborted.");
                        }
                    } else {
                        break signal;
                    }
                }
                // Trigger graceful shutdown if a component crashed, or all sources have ended.
                _ = graceful_crash.next() => break SignalTo::Shutdown,
                _ = &mut sources_finished => break SignalTo::Shutdown,
                else => unreachable!("Signal streams never end"),
            }
        };

        match signal {
            SignalTo::Shutdown => {
                emit!(VectorStopped);
                select! {
                    _ = topology.stop().compat() => (), // Graceful shutdown finished
                    _ = signals.next() => {
                        info!("Shutting down immediately.");
                        // Dropping the shutdown future will immediately shut the server down
                    }
                }
            }
            SignalTo::Quit => {
                info!("Shutting down immediately");
                drop(topology);
            }
            SignalTo::Reload => unreachable!(),
        }
    });

    rt.shutdown_now().wait().unwrap();
}

fn handle_config_errors(config: Result<Config, Vec<String>>) -> Option<Config> {
    match config {
        Err(errors) => {
            for error in errors {
                error!("Configuration error: {}", error);
            }
            None
        }
        Ok(config) => Some(config),
    }
}
