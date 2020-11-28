package metadata

components: sources: kubernetes_logs: {
	_directory: "/var/log"

	title: "Kubernetes Logs"

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		deployment_roles: ["daemon"]
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		collect: {
			checkpoint: enabled: true
			from: {
				service: {
					name:     "Kubernetes"
					thing:    "a \(name) cluster"
					url:      urls.kubernetes
					versions: ">= 1.14"
				}

				interface: {
					file_system: {
						directory: _directory
					}
				}
			}
		}
		multiline: enabled: false
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: "kubernetes"
	}

	configuration: {
		annotation_fields: {
			common:      false
			description: "Configuration for how the events are annotated with Pod metadata."
			required:    false
			type: object: {
				examples: []
				options: {
					container_image: {
						common:      false
						description: "Event field for Container image."
						required:    false
						type: string: {
							default: "kubernetes.container_image"
						}
					}
					container_name: {
						common:      false
						description: "Event field for Container name."
						required:    false
						type: string: {
							default: "kubernetes.container_name"
						}
					}
					pod_labels: {
						common:      false
						description: "Event field for Pod labels."
						required:    false
						type: string: {
							default: "kubernetes.pod_labels"
						}
					}
					pod_name: {
						common:      false
						description: "Event field for Pod name."
						required:    false
						type: string: {
							default: "kubernetes.pod_name"
						}
					}
					pod_namespace: {
						common:      false
						description: "Event field for Pod namespace."
						required:    false
						type: string: {
							default: "kubernetes.pod_namespace"
						}
					}
					pod_node_name: {
						common:      false
						description: "Event field for Pod node_name."
						required:    false
						type: string: {
							default: "kubernetes.pod_node_name"
						}
					}
					pod_uid: {
						common:      false
						description: "Event field for Pod uid."
						required:    false
						type: string: {
							default: "kubernetes.pod_uid"
						}
					}
				}
			}
		}
		auto_partial_merge: {
			common:      false
			description: "Automatically merge partial messages into a single event. Partial here is in respect to messages that were split by the Kubernetes Container Runtime log driver."
			required:    false
			type: bool: default: true
		}
		self_node_name: {
			common:      false
			description: "The name of the Kubernetes `Node` this Vector instance runs at. Configured to use an env var by default, to be evaluated to a value provided by Kubernetes at Pod deploy time."
			required:    false
			type: string: {
				default: "${VECTOR_SELF_NODE_NAME}"
			}
		}
		exclude_paths_glob_patterns: {
			common: false
			description: """
				A list of glob patterns to exclude from reading the files.
				"""
			required: false
			type: array: {
				default: []
				items: type: string: examples: ["**/exclude/**"]
			}
		}
		extra_field_selector: {
			common: false
			description: """
				Specifies the field selector to filter `Pod`s with, to be used in addition to the built-in `Node` filter.
				The name of the Kubernetes `Node` this Vector instance runs at. Configured to use an env var by default, to be evaluated to a value provided by Kubernetes at Pod deploy time.
				"""
			required: false
			type: array: {
				default: []
				items: type: string: examples: ["metadata.name!=pod-name-to-exclude"]
			}
		}
		extra_label_selector: {
			common: false
			description: """
				Specifies the label selector to filter `Pod`s with, to be used in
				addition to the built-in `vector.dev/exclude` filter.
				"""
			required: false
			type: array: {
				default: []
				items: type: string: examples: ["my_custom_label!=my_value"]
			}
		}
	}

	output: logs: line: {
		description: "An individual line from a `Pod` log file."
		fields: {
			file: {
				description: "The absolute path of originating file."
				required:    true
				type: string: examples: ["\(_directory)/pods/pod-namespace_pod-name_pod-uid/container/1.log"]
			}
			"kubernetes.container_image": {
				description: "Container image."
				required:    false
				common:      true
				type: string: {
					examples: ["busybox:1.30"]
					default: null
				}
			}
			"kubernetes.container_name": {
				description: "Container name."
				required:    false
				common:      true
				type: string: {
					examples: ["coredns"]
					default: null
				}
			}
			"kubernetes.pod_labels": {
				description: "Pod labels name."
				required:    false
				common:      true
				type: object: {
					examples: [{"mylabel": "myvalue"}]
					options: {}
				}
			}
			"kubernetes.pod_name": {
				description: "Pod name."
				required:    false
				common:      true
				type: string: {
					examples: ["coredns-qwertyuiop-qwert"]
					default: null
				}
			}
			"kubernetes.pod_namespace": {
				description: "Pod namespace."
				required:    false
				common:      true
				type: string: {
					examples: ["kube-system"]
					default: null
				}
			}
			"kubernetes.pod_node_name": {
				description: "Pod node name."
				required:    false
				common:      true
				type: string: {
					examples: ["minikube"]
					default: null
				}
			}
			"kubernetes.pod_uid": {
				description: "Pod uid."
				required:    false
				common:      true
				type: string: {
					examples: ["ba46d8c9-9541-4f6b-bbf9-d23b36f2f136"]
					default: null
				}
			}
			message: {
				description: "The raw line from the Pod log file."
				required:    true
				type: string: examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: examples: ["kubernetes_logs"]
			}
			stream: {
				description: "The name of the stream the log line was sumbitted to."
				required:    true
				type: string: examples: ["stdout", "stderr"]
			}
			timestamp: fields._current_timestamp
		}
	}

	examples: [
		{
			title: "Sample Output"
			configuration: {}
			input: """
				```text
				F1015 11:01:46.499073       1 main.go:39] error getting server version: Get \"https://10.96.0.1:443/version?timeout=32s\": dial tcp 10.96.0.1:443: connect: network is unreachable
				```
				"""
			output: log: {
				"file":                       "/var/log/pods/kube-system_storage-provisioner_93bde4d0-9731-4785-a80e-cd27ba8ad7c2/storage-provisioner/1.log"
				"kubernetes.container_image": "gcr.io/k8s-minikube/storage-provisioner:v3"
				"kubernetes.container_name":  "storage-provisioner"
				"kubernetes.pod_labels": {
					"addonmanager.kubernetes.io/mode": "Reconcile"
					"gcp-auth-skip-secret":            "true"
					"integration-test":                "storage-provisioner"
				}
				"kubernetes.pod_name":      "storage-provisioner"
				"kubernetes.pod_namespace": "kube-system"
				"kubernetes.pod_node_name": "minikube"
				"kubernetes.pod_uid":       "93bde4d0-9731-4785-a80e-cd27ba8ad7c2"
				"message":                  "F1015 11:01:46.499073       1 main.go:39] error getting server version: Get \"https://10.96.0.1:443/version?timeout=32s\": dial tcp 10.96.0.1:443: connect: network is unreachable"
				"source_type":              "kubernetes_logs"
				"stream":                   "stderr"
				"timestamp":                "2020-10-15T11:01:46.499555308Z"
			}
		},
	]

	// Note that these sections are also copied over the
	// `installation.platforms.kubernetes.how_it_works` key. Therefore, full
	// URLs should be used in links and language should be used that works in
	// both contexts.
	how_it_works: {
		enrichment: {
			title: "Enrichment"
			body:  """
					Vector will enrich data with Kubernetes context. A comprehensive
					list of fields can be found in the
					[`kubernetes_logs` source output docs](\(urls.vector_kubernetes_logs_source)#output)
					for a comprehensive list of fields.
					"""
		}

		filtering: {
			title: "Filtering"
			body:  """
					Please refer to the [`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source)#output)
					docs for filtering options.
					"""
		}

		pod_exclusion: {
			title: "Pod exclusion"
			body:  """
					By default, [`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source))
					will skip logs from the `Pod`s that have
					a `vector.dev/exclude: "true"` *label*.
					You can configure additional exclusion rules via label or field selectors,
					see [the available options](urls.vector_kubernetes_logs_source)#configuration).
					"""
		}

		container_exclusion: {
			title: "Container exclusion"
			body:  """
					[`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source))
					can skip the logs from the individual `container`s of a particular `Pod`.
					Add an *annotation* `vector.dev/exclude-containers` to
					the `Pod`, and enumerate the `name`s of all the `container`s
					to exclude in the value of the annotation like so:
					`vector.dev/exclude-containers: "container1,container2"`.
					This annotation will make Vector skip logs originating from
					the `container1` and `container2` of the `Pod` marked with
					the annotation, while logs from other `container`s in
					the `Pod` will still be collected.
					"""
		}

		kubernetes_api_communication: {
			title: "Kubernetes API communication"
			body:  """
					Vector communicates with the Kubernetes API to enrich the data it collects with
					Kubernetes context. Therefore, Vector must have access to communicate with the
					[Kubernetes API server](\(urls.kubernetes_api_server)). If Vector is running in
					a Kubernetes cluster then Vector will connect to that cluster using the
					[Kubernetes provided access information](\(urls.kubernetes_accessing_api_from_pod)).

					In addition to access, Vector implements proper desync handling to ensure
					communication is safe and reliable. This ensures that Vector will not overwhelm
					the Kubernetes API or compromise its stability.
					"""
		}

		partial_message_merging: {
			title: "Partial message merging"
			body:  """
					Vector, by default, will merge partial messages that are split due to the Docker
					size limit. For everything else, the
					[`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source)) offers `multiline`
					options to configure custom merging to handle merging things like stacktraces.
					"""
		}

		pod_removal: {
			title: "Pod removal"
			body: """
				To ensure all data is collected, Vector will continue to collect logs from the
				`Pod` for some time after its removal. This ensures that Vector obtains some of
				the most important data, such as crash details.
				"""
		}

		state_management: {
			title: "State management"
			body: """
				For the agent role, Vector stores its state at the host-mapped dir with a static
				path, so if it's redeployed it'll continue from where it was interrupted.
				"""
		}

		testing_and_reliability: {
			title: "Testing & reliability"
			body: """
				Vector is tested extensively against Kubernetes. In addition to Kubernetes
				being Vector's most popular installation method, Vector implements a
				comprehensive end-to-end test suite for all minor Kubernetes versions starting
				with `1.14.
				"""
		}

		kubernetes_api_access_control: {
			title: "Kubernetes API access control"
			body:  """
				Vector requires access to the Kubernetes API.
				Specifically, the [`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source))
				uses the `/api/v1/pods` endpoint to "watch" the pods from
				all namespaces.

				Modern Kubernetes clusters run with RBAC
				(role-based access control) scheme.
				RBAC-enabled clusters require some configuration to grant Vector
				the authorization to access the Kubernetes API endpoints.
				As RBAC is currently the standard way of controlling access to
				the Kubernetes API, we ship the necessary configuration
				out of the box: see `ClusterRole`, `ClusterRoleBinding` and
				a `ServiceAccount` in our `kubectl` YAML config,
				and the `rbac` configuration at the Helm chart.

				If your cluster doesn't use any access control scheme
				and doesn't restrict access to the Kubernetes API,
				you don't need to do any extra configuration - Vector will
				just work.
				Clusters using legacy ABAC scheme are not officially supported
				(although Vector might work if you configure access properly) -
				we encourage switching to RBAC.
				If you use a custom access control scheme -
				make sure Vector `Pod`/`ServiceAccount` is granted access to
				the `/api/v1/pods` resource.
				"""
		}
	}

	telemetry: metrics: {
		k8s_format_picker_edge_cases_total:     components.sources.internal_metrics.output.metrics.k8s_format_picker_edge_cases_total
		k8s_docker_format_parse_failures_total: components.sources.internal_metrics.output.metrics.k8s_docker_format_parse_failures_total
		k8s_event_annotation_failures_total:    components.sources.internal_metrics.output.metrics.k8s_event_annotation_failures_total
	}
}
