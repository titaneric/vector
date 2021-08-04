use crate::topology::builder;
use crate::topology::fanout::{ControlChannel, ControlMessage};
use crate::topology::{
    build_or_log_errors, handle_errors, retain, take_healthchecks, BuiltBuffer, Outputs,
    TaskHandle, WatchRx, WatchTx,
};
use crate::{
    buffers,
    config::{Config, ConfigDiff, HealthcheckOptions, Resource},
    event::Event,
    shutdown::SourceShutdownCoordinator,
    topology::{builder::Pieces, task::TaskOutput},
    trigger::DisabledTrigger,
};
use futures::{future, Future, FutureExt, SinkExt};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio::{
    sync::{mpsc, watch},
    time::{interval, sleep_until, Duration, Instant},
};
use tracing::Instrument;

#[allow(dead_code)]
pub struct RunningTopology {
    inputs: HashMap<String, buffers::BufferInputCloner<Event>>,
    outputs: HashMap<String, ControlChannel>,
    source_tasks: HashMap<String, TaskHandle>,
    tasks: HashMap<String, TaskHandle>,
    shutdown_coordinator: SourceShutdownCoordinator,
    detach_triggers: HashMap<String, DisabledTrigger>,
    pub(crate) config: Config,
    abort_tx: mpsc::UnboundedSender<()>,
    watch: (WatchTx, WatchRx),
}

impl RunningTopology {
    pub fn new(config: Config, abort_tx: mpsc::UnboundedSender<()>) -> Self {
        Self {
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            config,
            shutdown_coordinator: SourceShutdownCoordinator::default(),
            detach_triggers: HashMap::new(),
            source_tasks: HashMap::new(),
            tasks: HashMap::new(),
            abort_tx,
            watch: watch::channel(HashMap::new()),
        }
    }

    /// Signal that all sources in this topology are ended
    ///
    /// The future returned by this function will finish once all the sources in
    /// this topology have finished. This allows the caller to wait for or
    /// detect that the sources in the topology are no longer
    /// producing. [`vector::Application`], as an example, uses this as a
    /// shutdown signal.
    pub fn sources_finished(&self) -> future::BoxFuture<'static, ()> {
        self.shutdown_coordinator.shutdown_tripwire()
    }

    /// Shut down all topology components
    ///
    /// This function sends the shutdown signal to all sources in this topology
    /// and returns a future that resolves once all components (sources,
    /// transforms, and sinks) have finished shutting down. Transforms and sinks
    /// will shut down automatically once their input tasks finish.
    ///
    /// This function takes ownership of `self`, so once it returns everything
    /// in the [`RunningTopology`] instance has been dropped except for the
    /// `tasks` map. This map gets moved into the returned future and is used to
    /// poll for when the tasks have completed. Once the returned future is
    /// dropped then everything from this RunningTopology instance is fully
    /// dropped.
    pub fn stop(self) -> impl Future<Output = ()> {
        // Create handy handles collections of all tasks for the subsequent
        // operations.
        let mut wait_handles = Vec::new();
        // We need a Vec here since source components have two tasks. One for
        // pump in self.tasks, and the other for source in self.source_tasks.
        let mut check_handles = HashMap::<String, Vec<_>>::new();

        // We need to give some time to the sources to gracefully shutdown, so
        // we will merge them with other tasks.
        for (name, task) in self.tasks.into_iter().chain(self.source_tasks.into_iter()) {
            let task = task.map(|_result| ()).shared();

            wait_handles.push(task.clone());
            check_handles.entry(name).or_default().push(task);
        }

        // If we reach this, we will forcefully shutdown the sources.
        let deadline = Instant::now() + Duration::from_secs(60);

        // If we reach the deadline, this future will print out which components
        // won't gracefully shutdown since we will start to forcefully shutdown
        // the sources.
        let mut check_handles2 = check_handles.clone();
        let timeout = async move {
            sleep_until(deadline).await;
            // Remove all tasks that have shutdown.
            check_handles2.retain(|_name, handles| {
                retain(handles, |handle| handle.peek().is_none());
                !handles.is_empty()
            });
            let remaining_components = check_handles2.keys().cloned().collect::<Vec<_>>();

            error!(
              message = "Failed to gracefully shut down in time. Killing components.",
                components = ?remaining_components.join(", ")
            );
        };

        // Reports in intervals which components are still running.
        let mut interval = interval(Duration::from_secs(5));
        let reporter = async move {
            loop {
                interval.tick().await;
                // Remove all tasks that have shutdown.
                check_handles.retain(|_name, handles| {
                    retain(handles, |handle| handle.peek().is_none());
                    !handles.is_empty()
                });
                let remaining_components = check_handles.keys().cloned().collect::<Vec<_>>();

                let time_remaining = match deadline.checked_duration_since(Instant::now()) {
                    Some(remaining) => format!("{} seconds left", remaining.as_secs()),
                    None => "overdue".to_string(),
                };

                info!(
                    message = "Shutting down... Waiting on running components.", remaining_components = ?remaining_components.join(", "), time_remaining = ?time_remaining
                );
            }
        };

        // Finishes once all tasks have shutdown.
        let success = futures::future::join_all(wait_handles).map(|_| ());

        // Aggregate future that ends once anything detects that all tasks have
        // shutdown.
        let shutdown_complete_future = future::select_all(vec![
            Box::pin(timeout) as future::BoxFuture<'static, ()>,
            Box::pin(reporter) as future::BoxFuture<'static, ()>,
            Box::pin(success) as future::BoxFuture<'static, ()>,
        ]);

        // Now kick off the shutdown process by shutting down the sources.
        let source_shutdown_complete = self.shutdown_coordinator.shutdown_all(deadline);

        futures::future::join(source_shutdown_complete, shutdown_complete_future).map(|_| ())
    }

    /// On Error, topology is in invalid state.
    /// May change componenets even if reload fails.
    pub async fn reload_config_and_respawn(&mut self, new_config: Config) -> Result<bool, ()> {
        if self.config.global != new_config.global {
            error!(
                message =
                "Global options can't be changed while reloading config file; reload aborted. Please restart vector to reload the configuration file."
            );
            return Ok(false);
        }

        let diff = ConfigDiff::new(&self.config, &new_config);

        // Checks passed so let's shutdown the difference.
        let buffers = self.shutdown_diff(&diff, &new_config).await;

        // Gives windows some time to make available any port
        // released by shutdown componenets.
        // Issue: https://github.com/timberio/vector/issues/3035
        if cfg!(windows) {
            // This value is guess work.
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        // Now let's actually build the new pieces.
        if let Some(mut new_pieces) = build_or_log_errors(&new_config, &diff, buffers.clone()).await
        {
            if self
                .run_healthchecks(&diff, &mut new_pieces, new_config.healthchecks)
                .await
            {
                self.connect_diff(&diff, &mut new_pieces).await;
                self.spawn_diff(&diff, new_pieces);
                self.config = new_config;
                // We have successfully changed to new config.
                return Ok(true);
            }
        }

        // We need to rebuild the removed.
        info!("Rebuilding old configuration.");
        let diff = diff.flip();
        if let Some(mut new_pieces) = build_or_log_errors(&self.config, &diff, buffers).await {
            if self
                .run_healthchecks(&diff, &mut new_pieces, self.config.healthchecks)
                .await
            {
                self.connect_diff(&diff, &mut new_pieces).await;
                self.spawn_diff(&diff, new_pieces);
                // We have successfully returned to old config.
                return Ok(false);
            }
        }

        // We failed in rebuilding the old state.
        error!("Failed in rebuilding the old configuration.");

        Err(())
    }

    pub(crate) async fn run_healthchecks(
        &mut self,
        diff: &ConfigDiff,
        pieces: &mut Pieces,
        options: HealthcheckOptions,
    ) -> bool {
        if options.enabled {
            let healthchecks = take_healthchecks(diff, pieces)
                .into_iter()
                .map(|(_, task)| task);
            let healthchecks = future::try_join_all(healthchecks);

            info!("Running healthchecks.");
            if options.require_healthy {
                let success = healthchecks.await;

                if success.is_ok() {
                    info!("All healthchecks passed.");
                    true
                } else {
                    error!("Sinks unhealthy.");
                    false
                }
            } else {
                tokio::spawn(healthchecks);
                true
            }
        } else {
            true
        }
    }

    /// Shutdowns removed and replaced pieces of topology.
    /// Returns buffers to be reused.
    async fn shutdown_diff(
        &mut self,
        diff: &ConfigDiff,
        new_config: &Config,
    ) -> HashMap<String, BuiltBuffer> {
        // Sources
        let timeout = Duration::from_secs(30); //sec

        // First pass to tell the sources to shut down.
        let mut source_shutdown_complete_futures = Vec::new();

        // Only log that we are waiting for shutdown if we are actually removing
        // sources.
        if !diff.sources.to_remove.is_empty() {
            info!(
                message = "Waiting for sources to finish shutting down.", timeout = ?timeout.as_secs()
            );
        }

        let deadline = Instant::now() + timeout;
        for name in &diff.sources.to_remove {
            info!(message = "Removing source.", name = ?name);

            let previous = self.tasks.remove(name).unwrap();
            drop(previous); // detach and forget

            self.remove_outputs(name);
            source_shutdown_complete_futures
                .push(self.shutdown_coordinator.shutdown_source(name, deadline));
        }
        for name in &diff.sources.to_change {
            self.remove_outputs(name);
            source_shutdown_complete_futures
                .push(self.shutdown_coordinator.shutdown_source(name, deadline));
        }

        // Wait for the shutdowns to complete

        // Only log message if there are actual futures to check.
        if !source_shutdown_complete_futures.is_empty() {
            info!(
                "Waiting for up to {} seconds for sources to finish shutting down.",
                timeout.as_secs()
            );
        }

        futures::future::join_all(source_shutdown_complete_futures).await;

        // Second pass now that all sources have shut down for final cleanup.
        for name in diff.sources.removed_and_changed() {
            if let Some(task) = self.source_tasks.remove(name) {
                task.await.unwrap().unwrap();
            }
        }

        // Transforms
        for name in &diff.transforms.to_remove {
            info!(message = "Removing transform.", name = ?name);

            let previous = self.tasks.remove(name).unwrap();
            drop(previous); // detach and forget

            self.remove_inputs(name).await;
            self.remove_outputs(name);
        }

        // Sinks

        // Resource conflicts
        // At this point both the old and the new config don't have
        // conflicts in their resource usage. So if we combine their
        // resources, all found conflicts are between
        // to be removed and to be added components.
        let remove_sink = diff
            .sinks
            .removed_and_changed()
            .map(|name| (name, self.config.sinks[name].resources(name)));
        let add_source = diff
            .sources
            .changed_and_added()
            .map(|name| (name, new_config.sources[name].inner.resources()));
        let add_sink = diff
            .sinks
            .changed_and_added()
            .map(|name| (name, new_config.sinks[name].resources(name)));
        let conflicts = Resource::conflicts(
            remove_sink.map(|(key, value)| ((true, key), value)).chain(
                add_sink
                    .chain(add_source)
                    .map(|(key, value)| ((false, key), value)),
            ),
        )
        .into_iter()
        .flat_map(|(_, components)| components)
        .collect::<HashSet<_>>();
        // Existing conflicting sinks
        let conflicting_sinks = conflicts
            .into_iter()
            .filter(|&(existing_sink, _)| existing_sink)
            .map(|(_, name)| name.clone());

        // Buffer reuse
        // We can reuse buffers whose configuration wasn't changed.
        let reuse_buffers = diff
            .sinks
            .to_change
            .iter()
            .filter(|&name| self.config.sinks[name].buffer == new_config.sinks[name].buffer)
            .cloned()
            .collect::<HashSet<_>>();

        let wait_for_sinks = conflicting_sinks
            .chain(reuse_buffers.iter().cloned())
            .collect::<HashSet<_>>();

        // First pass

        // Detach removed sinks
        for name in &diff.sinks.to_remove {
            info!(message = "Removing sink.", name = ?name);
            self.remove_inputs(name).await;
        }

        // Detach changed sinks
        for name in &diff.sinks.to_change {
            if reuse_buffers.contains(name) {
                self.detach_triggers
                    .remove(name)
                    .unwrap()
                    .into_inner()
                    .cancel();
            } else if wait_for_sinks.contains(name) {
                self.detach_inputs(name).await;
            }
        }

        // Second pass for final cleanup

        // Cleanup removed
        for name in &diff.sinks.to_remove {
            let previous = self.tasks.remove(name).unwrap();
            if wait_for_sinks.contains(name) {
                debug!(message = "Waiting for sink to shutdown.", %name);
                previous.await.unwrap().unwrap();
            } else {
                drop(previous); // detach and forget
            }
        }

        // Cleanup changed and collect buffers to be reused
        let mut buffers = HashMap::new();
        for name in &diff.sinks.to_change {
            if wait_for_sinks.contains(name) {
                let previous = self.tasks.remove(name).unwrap();
                debug!(message = "Waiting for sink to shutdown.", %name);
                let buffer = previous.await.unwrap().unwrap();

                if reuse_buffers.contains(name) {
                    let tx = self.inputs.remove(name).unwrap();
                    let (rx, acker) = match buffer {
                        TaskOutput::Sink(rx, acker) => (rx, acker),
                        _ => unreachable!(),
                    };

                    buffers.insert(name.clone(), (tx, Arc::new(Mutex::new(Some(rx))), acker));
                }
            }
        }

        buffers
    }

    /// Rewires topology
    pub(crate) async fn connect_diff(&mut self, diff: &ConfigDiff, new_pieces: &mut Pieces) {
        // Sources
        for name in diff.sources.changed_and_added() {
            self.setup_outputs(name, new_pieces).await;
        }

        // Transforms
        // Make sure all transform outputs are set up before another transform
        // might try use it as an input
        for name in diff.transforms.changed_and_added() {
            self.setup_outputs(name, new_pieces).await;
        }

        for name in &diff.transforms.to_change {
            self.replace_inputs(name, new_pieces).await;
        }

        for name in &diff.transforms.to_add {
            self.setup_inputs(name, new_pieces).await;
        }

        // Sinks
        for name in &diff.sinks.to_change {
            self.replace_inputs(name, new_pieces).await;
        }

        for name in &diff.sinks.to_add {
            self.setup_inputs(name, new_pieces).await;
        }

        // Broadcast changes to subscribers.
        if !self.watch.0.is_closed() {
            self.watch
                .0
                .send(self.outputs.clone())
                .expect("Couldn't broadcast config changes.");
        }
    }

    /// Starts new and changed pieces of topology.
    pub(crate) fn spawn_diff(&mut self, diff: &ConfigDiff, mut new_pieces: Pieces) {
        // Sources
        for name in &diff.sources.to_change {
            info!(message = "Rebuilding source.", name = ?name);
            self.spawn_source(name, &mut new_pieces);
        }

        for name in &diff.sources.to_add {
            info!(message = "Starting source.", name = ?name);
            self.spawn_source(name, &mut new_pieces);
        }

        // Transforms
        for name in &diff.transforms.to_change {
            info!(message = "Rebuilding transform.", name = ?name);
            self.spawn_transform(name, &mut new_pieces);
        }

        for name in &diff.transforms.to_add {
            info!(message = "Starting transform.", name = ?name);
            self.spawn_transform(name, &mut new_pieces);
        }

        // Sinks
        for name in &diff.sinks.to_change {
            info!(message = "Rebuilding sink.", name = ?name);
            self.spawn_sink(name, &mut new_pieces);
        }

        for name in &diff.sinks.to_add {
            info!(message = "Starting sink.", name = ?name);
            self.spawn_sink(name, &mut new_pieces);
        }
    }

    fn spawn_sink(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let task = new_pieces.tasks.remove(name).unwrap();
        let span = error_span!(
            "sink",
            component_kind = "sink",
            component_name = %task.name(),
            component_type = %task.typetag(),
        );
        let task = handle_errors(task, self.abort_tx.clone()).instrument(span);
        let spawned = tokio::spawn(task);
        if let Some(previous) = self.tasks.insert(name.to_string(), spawned) {
            drop(previous); // detach and forget
        }
    }

    fn spawn_transform(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let task = new_pieces.tasks.remove(name).unwrap();
        let span = error_span!(
            "transform",
            component_kind = "transform",
            component_name = %task.name(),
            component_type = %task.typetag(),
        );
        let task = handle_errors(task, self.abort_tx.clone()).instrument(span);
        let spawned = tokio::spawn(task);
        if let Some(previous) = self.tasks.insert(name.to_string(), spawned) {
            drop(previous); // detach and forget
        }
    }

    fn spawn_source(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let task = new_pieces.tasks.remove(name).unwrap();
        let span = error_span!(
            "source",
            component_kind = "source",
            component_name = %task.name(),
            component_type = %task.typetag(),
        );
        let task = handle_errors(task, self.abort_tx.clone()).instrument(span.clone());
        let spawned = tokio::spawn(task);
        if let Some(previous) = self.tasks.insert(name.to_string(), spawned) {
            drop(previous); // detach and forget
        }

        self.shutdown_coordinator
            .takeover_source(name, &mut new_pieces.shutdown_coordinator);

        let source_task = new_pieces.source_tasks.remove(name).unwrap();
        let source_task = handle_errors(source_task, self.abort_tx.clone()).instrument(span);
        self.source_tasks
            .insert(name.to_string(), tokio::spawn(source_task));
    }

    fn remove_outputs(&mut self, name: &str) {
        self.outputs.remove(name);
    }

    async fn remove_inputs(&mut self, name: &str) {
        self.inputs.remove(name);
        self.detach_triggers.remove(name);

        let sink_inputs = self.config.sinks.get(name).map(|s| &s.inputs);
        let trans_inputs = self.config.transforms.get(name).map(|t| &t.inputs);

        let inputs = sink_inputs.or(trans_inputs);

        if let Some(inputs) = inputs {
            for input in inputs {
                if let Some(output) = self.outputs.get_mut(input) {
                    // This can only fail if we are disconnected, which is a valid situation.
                    let _ = output.send(ControlMessage::Remove(name.to_string())).await;
                }
            }
        }
    }

    async fn setup_outputs(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let mut output = new_pieces.outputs.remove(name).unwrap();

        for (sink_name, sink) in &self.config.sinks {
            if sink.inputs.iter().any(|i| i == name) {
                // Sink may have been removed with the new config so it may not
                // be present.
                if let Some(input) = self.inputs.get(sink_name) {
                    let _ = output
                        .send(ControlMessage::Add(sink_name.clone(), input.get()))
                        .await;
                }
            }
        }
        for (transform_name, transform) in &self.config.transforms {
            if transform.inputs.iter().any(|i| i == name) {
                // Transform may have been removed with the new config so it may
                // not be present.
                if let Some(input) = self.inputs.get(transform_name) {
                    let _ = output
                        .send(ControlMessage::Add(transform_name.clone(), input.get()))
                        .await;
                }
            }
        }

        self.outputs.insert(name.to_string(), output);
    }

    async fn setup_inputs(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let (tx, inputs) = new_pieces.inputs.remove(name).unwrap();

        for input in inputs {
            // This can only fail if we are disconnected, which is a valid situation.
            let _ = self
                .outputs
                .get_mut(&input)
                .unwrap()
                .send(ControlMessage::Add(name.to_string(), tx.get()))
                .await;
        }

        self.inputs.insert(name.to_string(), tx);
        new_pieces.detach_triggers.remove(name).map(|trigger| {
            self.detach_triggers
                .insert(name.to_string(), trigger.into())
        });
    }

    async fn replace_inputs(&mut self, name: &str, new_pieces: &mut builder::Pieces) {
        let (tx, inputs) = new_pieces.inputs.remove(name).unwrap();

        let sink_inputs = self.config.sinks.get(name).map(|s| &s.inputs);
        let trans_inputs = self.config.transforms.get(name).map(|t| &t.inputs);
        let old_inputs = sink_inputs
            .or(trans_inputs)
            .unwrap()
            .iter()
            .collect::<HashSet<_>>();

        let new_inputs = inputs.iter().collect::<HashSet<_>>();

        let inputs_to_remove = &old_inputs - &new_inputs;
        let inputs_to_add = &new_inputs - &old_inputs;
        let inputs_to_replace = old_inputs.intersection(&new_inputs);

        for input in inputs_to_remove {
            if let Some(output) = self.outputs.get_mut(input) {
                // This can only fail if we are disconnected, which is a valid situation.
                let _ = output.send(ControlMessage::Remove(name.to_string())).await;
            }
        }

        for input in inputs_to_add {
            // This can only fail if we are disconnected, which is a valid situation.
            let _ = self
                .outputs
                .get_mut(input)
                .unwrap()
                .send(ControlMessage::Add(name.to_string(), tx.get()))
                .await;
        }

        for &input in inputs_to_replace {
            // This can only fail if we are disconnected, which is a valid situation.
            let _ = self
                .outputs
                .get_mut(input)
                .unwrap()
                .send(ControlMessage::Replace(name.to_string(), Some(tx.get())))
                .await;
        }

        self.inputs.insert(name.to_string(), tx);
        new_pieces.detach_triggers.remove(name).map(|trigger| {
            self.detach_triggers
                .insert(name.to_string(), trigger.into())
        });
    }

    async fn detach_inputs(&mut self, name: &str) {
        self.inputs.remove(name);
        self.detach_triggers.remove(name);

        let sink_inputs = self.config.sinks.get(name).map(|s| &s.inputs);
        let trans_inputs = self.config.transforms.get(name).map(|t| &t.inputs);
        let old_inputs = sink_inputs.or(trans_inputs).unwrap();

        for input in old_inputs {
            // This can only fail if we are disconnected, which is a valid
            // situation.
            let _ = self
                .outputs
                .get_mut(input)
                .unwrap()
                .send(ControlMessage::Replace(name.to_string(), None))
                .await;
        }
    }

    /// Borrows the Config
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Subscribe to topology changes. This will receive an `Outputs` currently, but may be
    /// expanded in the future to accommodate `Inputs`. This is used by the 'tap' API to observe
    /// config changes, and re-wire tap sinks.
    pub fn watch(&self) -> watch::Receiver<Outputs> {
        self.watch.1.clone()
    }
}
