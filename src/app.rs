use crate::cli::{handle_config_errors, Color, LogFormat, Opts, RootOpts, SubCommand};
use crate::signal::SignalTo;
use crate::topology::RunningTopology;
use crate::{
    config, generate, heartbeat, list, metrics, signal, topology, trace, unit_test, validate,
};
use std::cmp::max;
use std::path::PathBuf;

use futures::{
    compat::{Future01CompatExt, Stream01CompatExt},
    StreamExt,
};
use futures01::sync::mpsc;

#[cfg(feature = "api")]
use crate::{api, internal_events::ApiStarted};

#[cfg(windows)]
use crate::service;

use crate::internal_events::{
    VectorConfigLoadFailed, VectorQuit, VectorRecoveryFailed, VectorReloadFailed, VectorReloaded,
    VectorStarted, VectorStopped,
};
use tokio::runtime;
use tokio::runtime::Runtime;

pub struct ApplicationConfig {
    pub config_paths: Vec<PathBuf>,
    pub topology: RunningTopology,
    pub graceful_crash: mpsc::UnboundedReceiver<()>,
    #[cfg(feature = "api")]
    pub api: config::api::Options,
}

pub struct Application {
    opts: RootOpts,
    pub config: ApplicationConfig,
    pub runtime: Runtime,
}

impl Application {
    pub fn prepare() -> Result<Self, exitcode::ExitCode> {
        let opts = Opts::get_matches();
        Self::prepare_from_opts(opts)
    }

    pub fn prepare_from_opts(opts: Opts) -> Result<Self, exitcode::ExitCode> {
        openssl_probe::init_ssl_cert_env_vars();

        let level = opts.log_level();
        let root_opts = opts.root;

        let sub_command = opts.sub_command;

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

        let color = match root_opts.color {
            #[cfg(unix)]
            Color::Auto => atty::is(atty::Stream::Stdout),
            #[cfg(windows)]
            Color::Auto => false, // ANSI colors are not supported by cmd.exe
            Color::Always => true,
            Color::Never => false,
        };

        let json = match &root_opts.log_format {
            LogFormat::Text => false,
            LogFormat::Json => true,
        };

        trace::init(color, json, levels.as_str());

        metrics::init().expect("metrics initialization failed");

        info!("Log level {:?} is enabled.", level);

        if let Some(threads) = root_opts.threads {
            if threads < 1 {
                error!("The `threads` argument must be greater or equal to 1.");
                return Err(exitcode::CONFIG);
            }
        }

        let mut rt = {
            let threads = root_opts.threads.unwrap_or_else(|| max(1, num_cpus::get()));
            runtime::Builder::new()
                .threaded_scheduler()
                .enable_all()
                .core_threads(threads)
                .build()
                .expect("Unable to create async runtime")
        };

        let config = {
            let config_paths = root_opts.config_paths.clone();
            let watch_config = root_opts.watch_config;
            let require_healthy = root_opts.require_healthy;

            rt.block_on(async move {
                if let Some(s) = sub_command {
                    let code = match s {
                        SubCommand::Validate(v) => validate::validate(&v, color).await,
                        SubCommand::List(l) => list::cmd(&l),
                        SubCommand::Test(t) => unit_test::cmd(&t),
                        SubCommand::Generate(g) => generate::cmd(&g),
                        #[cfg(windows)]
                        SubCommand::Service(s) => service::cmd(&s),
                    };

                    return Err(code);
                };

                let config_paths = config::process_paths(&config_paths).ok_or(exitcode::CONFIG)?;

                if watch_config {
                    // Start listening for config changes immediately.
                    config::watcher::spawn_thread(&config_paths, None).or_else(|error| {
                        error!(message = "Unable to start config watcher.", %error);
                        Err(exitcode::CONFIG)
                    })?;
                }

                info!(
                    message = "Loading configs.",
                    path = ?config_paths
                );

                let config =
                    config::load_from_paths(&config_paths).map_err(handle_config_errors)?;

                config::LOG_SCHEMA
                    .set(config.global.log_schema.clone())
                    .expect("Couldn't set schema");

                let diff = config::ConfigDiff::initial(&config);
                let pieces = topology::build_or_log_errors(&config, &diff)
                    .await
                    .ok_or(exitcode::CONFIG)?;

                #[cfg(feature = "api")]
                let api = config.api;

                let result = topology::start_validated(config, diff, pieces, require_healthy).await;
                let (topology, graceful_crash) = result.ok_or(exitcode::CONFIG)?;

                Ok(ApplicationConfig {
                    config_paths,
                    topology,
                    graceful_crash,
                    #[cfg(feature = "api")]
                    api,
                })
            })
        }?;

        Ok(Application {
            opts: root_opts,
            config,
            runtime: rt,
        })
    }

    pub fn run(self) {
        let mut rt = self.runtime;

        let graceful_crash = self.config.graceful_crash;
        let mut topology = self.config.topology;

        let mut config_paths = self.config.config_paths;

        #[cfg(feature = "api")]
        // assigned to prevent the API terminating when falling out of scope
        let _api = if self.config.api.enabled {
            emit!(ApiStarted {
                addr: self.config.api.bind.unwrap(),
                playground: self.config.api.playground
            });

            Some(api::Server::start(self.config.api))
        } else {
            None
        };

        let opts = self.opts;

        rt.block_on(async move {
            emit!(VectorStarted);
            tokio::spawn(heartbeat::heartbeat());

            let signals = signal::signals();
            tokio::pin!(signals);
            let mut sources_finished = topology.sources_finished().compat();
            let mut graceful_crash = graceful_crash.compat();

            let signal = loop {
                tokio::select! {
                Some(signal) = signals.next() => {
                    if signal == SignalTo::Reload {
                        // Reload paths
                        config_paths = config::process_paths(&opts.config_paths).unwrap_or(config_paths);
                        // Reload config
                        let new_config = config::load_from_paths(&config_paths).map_err(handle_config_errors).ok();

                        if let Some(new_config) = new_config {
                            match topology
                                .reload_config_and_respawn(new_config, opts.require_healthy)
                                .await
                            {
                                Ok(true) =>  emit!(VectorReloaded { config_paths: &config_paths }),
                                Ok(false) => emit!(VectorReloadFailed),
                                // Trigger graceful shutdown for what remains of the topology
                                Err(()) => {
                                    emit!(VectorReloadFailed);
                                    emit!(VectorRecoveryFailed);
                                    break SignalTo::Shutdown;
                                }
                            }
                            sources_finished = topology.sources_finished().compat();
                        } else {
                            emit!(VectorConfigLoadFailed);
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
                    tokio::select! {
                    _ = topology.stop().compat() => (), // Graceful shutdown finished
                    _ = signals.next() => {
                        // It is highly unlikely that this event will exit from topology.
                        emit!(VectorQuit);
                        // Dropping the shutdown future will immediately shut the server down
                    }
                }
                }
                SignalTo::Quit => {
                    // It is highly unlikely that this event will exit from topology.
                    emit!(VectorQuit);
                    drop(topology);
                }
                SignalTo::Reload => unreachable!(),
            }
        });
    }
}
