//! Annotates events with pod metadata.

#![deny(missing_docs)]

use super::path_helpers::{parse_log_file_path, LogFileInfo};
use crate::{
    event::{LogEvent, PathComponent, PathIter},
    kubernetes as k8s, Event,
};
use evmap::ReadHandle;
use k8s_openapi::{
    api::core::v1::{Container, Pod, PodSpec},
    apimachinery::pkg::apis::meta::v1::ObjectMeta,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct FieldsSpec {
    pub pod_name: String,
    pub pod_namespace: String,
    pub pod_uid: String,
    pub pod_labels: String,
    pub pod_node_name: String,
    pub container_name: String,
    pub container_image: String,
}

impl Default for FieldsSpec {
    fn default() -> Self {
        Self {
            pod_name: "kubernetes.pod_name".to_owned(),
            pod_namespace: "kubernetes.pod_namespace".to_owned(),
            pod_uid: "kubernetes.pod_uid".to_owned(),
            pod_labels: "kubernetes.pod_labels".to_owned(),
            pod_node_name: "kubernetes.pod_node_name".to_owned(),
            container_name: "kubernetes.container_name".to_owned(),
            container_image: "kubernetes.container_image".to_owned(),
        }
    }
}

/// Annotate the event with pod metadata.
pub struct PodMetadataAnnotator {
    pods_state_reader: ReadHandle<String, k8s::state::evmap::Value<Pod>>,
    fields_spec: FieldsSpec,
}

impl PodMetadataAnnotator {
    /// Create a new [`PodMetadataAnnotator`].
    pub fn new(
        pods_state_reader: ReadHandle<String, k8s::state::evmap::Value<Pod>>,
        fields_spec: FieldsSpec,
    ) -> Self {
        Self {
            pods_state_reader,
            fields_spec,
        }
    }
}

impl PodMetadataAnnotator {
    /// Annotates an event with the information from the [`Pod::metadata`].
    /// The event has to be obtained from kubernetes log file, and have a
    /// [`FILE_KEY`] field set with a file that the line came from.
    pub fn annotate(&self, event: &mut Event, file: &str) -> Option<()> {
        let log = event.as_mut_log();
        let file_info = parse_log_file_path(file)?;
        let guard = self.pods_state_reader.get(file_info.pod_uid)?;
        let entry = guard.get_one()?;
        let pod: &Pod = entry.as_ref();

        annotate_from_file_info(log, &self.fields_spec, &file_info);
        annotate_from_metadata(log, &self.fields_spec, &pod.metadata);
        if let Some(ref pod_spec) = pod.spec {
            annotate_from_pod_spec(log, &self.fields_spec, pod_spec);

            let container = pod_spec
                .containers
                .iter()
                .find(|c| c.name == file_info.container_name);
            if let Some(container) = container {
                annotate_from_container(log, &self.fields_spec, container);
            }
        }
        Some(())
    }
}

fn annotate_from_file_info(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    file_info: &LogFileInfo<'_>,
) {
    log.insert(
        &fields_spec.container_name,
        file_info.container_name.to_owned(),
    );
}

fn annotate_from_metadata(log: &mut LogEvent, fields_spec: &FieldsSpec, metadata: &ObjectMeta) {
    for (ref key, ref val) in [
        (&fields_spec.pod_name, &metadata.name),
        (&fields_spec.pod_namespace, &metadata.namespace),
        (&fields_spec.pod_uid, &metadata.uid),
    ]
    .iter()
    {
        if let Some(val) = val {
            log.insert(key, val.to_owned());
        }
    }

    if let Some(labels) = &metadata.labels {
        // Calculate and cache the prefix path.
        let prefix_path = PathIter::new(fields_spec.pod_labels.as_ref()).collect::<Vec<_>>();
        for (key, val) in labels.iter() {
            let mut path = prefix_path.clone();
            path.push(PathComponent::Key(key.clone()));
            log.insert_path(path, val.to_owned());
        }
    }
}

fn annotate_from_pod_spec(log: &mut LogEvent, fields_spec: &FieldsSpec, pod_spec: &PodSpec) {
    for (ref key, ref val) in [(&fields_spec.pod_node_name, &pod_spec.node_name)].iter() {
        if let Some(val) = val {
            log.insert(key, val.to_owned());
        }
    }
}

fn annotate_from_container(log: &mut LogEvent, fields_spec: &FieldsSpec, container: &Container) {
    for (ref key, ref val) in [(&fields_spec.container_image, &container.image)].iter() {
        if let Some(val) = val {
            log.insert(key, val.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotate_from_metadata() {
        let cases = vec![
            (
                FieldsSpec::default(),
                ObjectMeta::default(),
                LogEvent::default(),
            ),
            (
                FieldsSpec::default(),
                ObjectMeta {
                    name: Some("sandbox0-name".to_owned()),
                    namespace: Some("sandbox0-ns".to_owned()),
                    uid: Some("sandbox0-uid".to_owned()),
                    labels: Some(
                        vec![
                            ("sandbox0-label0".to_owned(), "val0".to_owned()),
                            ("sandbox0-label1".to_owned(), "val1".to_owned()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..ObjectMeta::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert("kubernetes.pod_name", "sandbox0-name");
                    log.insert("kubernetes.pod_namespace", "sandbox0-ns");
                    log.insert("kubernetes.pod_uid", "sandbox0-uid");
                    log.insert("kubernetes.pod_labels.sandbox0-label0", "val0");
                    log.insert("kubernetes.pod_labels.sandbox0-label1", "val1");
                    log
                },
            ),
            (
                FieldsSpec {
                    pod_name: "name".to_owned(),
                    pod_namespace: "ns".to_owned(),
                    pod_uid: "uid".to_owned(),
                    pod_labels: "labels".to_owned(),
                    ..Default::default()
                },
                ObjectMeta {
                    name: Some("sandbox0-name".to_owned()),
                    namespace: Some("sandbox0-ns".to_owned()),
                    uid: Some("sandbox0-uid".to_owned()),
                    labels: Some(
                        vec![
                            ("sandbox0-label0".to_owned(), "val0".to_owned()),
                            ("sandbox0-label1".to_owned(), "val1".to_owned()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..ObjectMeta::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert("name", "sandbox0-name");
                    log.insert("ns", "sandbox0-ns");
                    log.insert("uid", "sandbox0-uid");
                    log.insert("labels.sandbox0-label0", "val0");
                    log.insert("labels.sandbox0-label1", "val1");
                    log
                },
            ),
            // Ensure we properly handle labels with `.` as flat fields.
            (
                FieldsSpec::default(),
                ObjectMeta {
                    name: Some("sandbox0-name".to_owned()),
                    namespace: Some("sandbox0-ns".to_owned()),
                    uid: Some("sandbox0-uid".to_owned()),
                    labels: Some(
                        vec![
                            ("nested0.label0".to_owned(), "val0".to_owned()),
                            ("nested0.label1".to_owned(), "val1".to_owned()),
                            ("nested1.label0".to_owned(), "val2".to_owned()),
                            ("nested2.label0.deep0".to_owned(), "val3".to_owned()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..ObjectMeta::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert("kubernetes.pod_name", "sandbox0-name");
                    log.insert("kubernetes.pod_namespace", "sandbox0-ns");
                    log.insert("kubernetes.pod_uid", "sandbox0-uid");
                    log.insert("kubernetes.pod_labels.nested0\\.label0", "val0");
                    log.insert("kubernetes.pod_labels.nested0\\.label1", "val1");
                    log.insert("kubernetes.pod_labels.nested1\\.label0", "val2");
                    log.insert("kubernetes.pod_labels.nested2\\.label0\\.deep0", "val3");
                    log
                },
            ),
        ];

        for (fields_spec, metadata, expected) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_metadata(&mut log, &fields_spec, &metadata);
            assert_eq!(log, expected);
        }
    }

    #[test]
    fn test_annotate_from_file_info() {
        let cases = vec![(
            FieldsSpec::default(),
            "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/sandbox0-container0-name/1.log",
            {
                let mut log = LogEvent::default();
                log.insert("kubernetes.container_name", "sandbox0-container0-name");
                log
            },
        ),(
            FieldsSpec{
                container_name: "container_name".to_owned(),
                ..Default::default()
            },
            "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/sandbox0-container0-name/1.log",
            {
                let mut log = LogEvent::default();
                log.insert("container_name", "sandbox0-container0-name");
                log
            },
        )];

        for (fields_spec, file, expected) in cases.into_iter() {
            let mut log = LogEvent::default();
            let file_info = parse_log_file_path(file).unwrap();
            annotate_from_file_info(&mut log, &fields_spec, &file_info);
            assert_eq!(log, expected);
        }
    }

    #[test]
    fn test_annotate_from_pod_spec() {
        let cases = vec![
            (
                FieldsSpec::default(),
                PodSpec::default(),
                LogEvent::default(),
            ),
            (
                FieldsSpec::default(),
                PodSpec {
                    node_name: Some("sandbox0-node-name".to_owned()),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert("kubernetes.pod_node_name", "sandbox0-node-name");
                    log
                },
            ),
            (
                FieldsSpec {
                    pod_node_name: "node_name".to_owned(),
                    ..Default::default()
                },
                PodSpec {
                    node_name: Some("sandbox0-node-name".to_owned()),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert("node_name", "sandbox0-node-name");
                    log
                },
            ),
        ];

        for (fields_spec, pod_spec, expected) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_pod_spec(&mut log, &fields_spec, &pod_spec);
            assert_eq!(log, expected);
        }
    }

    #[test]
    fn test_annotate_from_container() {
        let cases = vec![
            (
                FieldsSpec::default(),
                Container::default(),
                LogEvent::default(),
            ),
            (
                FieldsSpec::default(),
                Container {
                    image: Some("sandbox0-container-image".to_owned()),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert("kubernetes.container_image", "sandbox0-container-image");
                    log
                },
            ),
            (
                FieldsSpec {
                    container_image: "container_image".to_owned(),
                    ..Default::default()
                },
                Container {
                    image: Some("sandbox0-container-image".to_owned()),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert("container_image", "sandbox0-container-image");
                    log
                },
            ),
        ];

        for (fields_spec, container, expected) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_container(&mut log, &fields_spec, &container);
            assert_eq!(log, expected);
        }
    }
}
