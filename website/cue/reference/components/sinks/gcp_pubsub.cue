package metadata

components: sinks: gcp_pubsub: {
	title: "GCP PubSub"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["GCP"]
		stateful: false
	}

	features: {
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				max_events:   1000
				timeout_secs: 1
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_enable:             false
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				service: services.gcp_pubsub

				interface: {
					socket: {
						api: {
							title: "GCP XML Interface"
							url:   urls.gcp_xml_interface
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		api_key: {
			common:      false
			description: "A [Google Cloud API key](\(urls.gcp_authentication_api_key)) used to authenticate access the pubsub project and topic. Either this or `credentials_path` must be set."
			required:    false
			type: string: {
				default: null
				examples: ["${GCP_API_KEY}", "ef8d5de700e7989468166c40fc8a0ccd"]
			}
		}
		credentials_path: {
			common:      true
			description: "The filename for a Google Cloud service account credentials JSON file used to authenticate access to the pubsub project and topic. If this is unset, Vector checks the `GOOGLE_APPLICATION_CREDENTIALS` environment variable for a filename.\n\nIf no filename is named, Vector will attempt to fetch an instance service account for the compute instance the program is running on. If Vector is not running on a GCE instance, you must define a credentials file as above."
			required:    false
			type: string: {
				default: null
				examples: ["/path/to/credentials.json"]
			}
		}
		endpoint: {
			common:      false
			description: "The endpoint to which to send data."
			required:    false
			type: string: {
				default: "https://pubsub.googleapis.com"
				examples: ["https://us-central1-pubsub.googleapis.com"]
			}
		}
		project: {
			description: "The project name to which to publish logs."
			required:    true
			type: string: {
				examples: ["vector-123456"]
			}
		}
		topic: {
			description: "The topic within the project to which to publish logs."
			required:    true
			type: string: {
				examples: ["this-is-a-topic"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	permissions: iam: [
		{
			platform: "gcp"
			_service: "pubsub"

			policies: [
				{
					_action: "topics.get"
					required_for: ["healthcheck"]
				},
				{
					_action: "topics.publish"
					required_for: ["operation"]
				},
			]
		},
	]

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
	}
}
