package metadata

components: sinks: axiom: {
	title: "Axiom"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Axiom"]
		stateful: false
	}

	features: {
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_events:   1000
				max_bytes:    1_048_576
				timeout_secs: 1.0
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: true
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				service: services.axiom

				interface: {
					socket: {
						api: {
							title: "Axiom API"
							url:   urls.axiom
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
		token: {
			description: "Your Axiom token"
			required:    true
			warnings: []
			type: string: {
				examples: ["xxxx", "${AXIOM_TOKEN}"]
				syntax: "literal"
			}
		}
		dataset: {
			description: "Your Axiom dataset"
			required:    true
			warnings: []
			type: string: {
				examples: ["vector.dev"]
				syntax: "literal"
			}
		}
		url: {
			description: "Your Axiom URL (only required if not Axiom Cloud)"
			common:      false
			required:    false
			warnings: []
			type: string: {
				examples: ["https://cloud.axiom.co", "${AXIOM_URL}"]
				syntax:  "literal"
				default: ""
			}
		}
		org_id: {
			description: "Your Axiom Org ID (only required for personal tokens)"
			common:      false
			required:    false
			warnings: []
			type: string: {
				examples: ["xxxx", "${AXIOM_ORG_ID}"]
				syntax:  "literal"
				default: ""
			}
		}
	}

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: false
	}

	how_it_works: {
		setup: {
			title: "Setup"
			body:  """
				1. Register for a free account at [cloud.axiom.co](\(urls.axiom_cloud))

				2. Once registered, create a new dataset and create an API token for it
				"""
		}
	}

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_discarded_total:           components.sources.internal_metrics.output.metrics.events_discarded_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
		processing_errors_total:          components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
