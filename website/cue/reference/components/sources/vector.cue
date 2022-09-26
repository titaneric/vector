package metadata

components: sources: vector: {
	_port: 9000

	title: "Vector"

	description: """
		Receives data from another upstream Vector instance using the Vector sink.
		"""

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		acknowledgements: true
		multiline: enabled: false
		receive: {
			from: {
				service: services.vector

				interface: socket: {
					direction: "incoming"
					port:      _port
					protocols: ["http"]
					ssl: "optional"
				}
			}
			receive_buffer_bytes: enabled: false
			keepalive: enabled:            true
			tls: {
				enabled:                true
				can_verify_certificate: true
				enabled_default:        false
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		acknowledgements: configuration._source_acknowledgements
		address: {
			description: """
				The HTTP address to listen for connections on. It _must_ include a port.
				"""
			required: true
			type: string: {
				examples: ["0.0.0.0:\(_port)"]
			}
		}
		version: {
			description: "Source API version. Specifying this version ensures that Vector does not silently break backward compatibility."
			common:      true
			required:    false
			warnings: ["Ensure you use the same version for both the source and sink."]
			type: string: {
				enum: {
					"2": "Vector source API version 2"
				}
				default: "2"
			}
		}
	}

	output: {
		logs: event: {
			description: "A Vector event"
			fields: {
				client_metadata: fields._client_metadata
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["vector"]
					}
				}
				"*": {
					description: "Vector transparently forwards data from another upstream Vector instance. The `vector` source will not modify or add fields."
					required:    true
					type: "*": {}
				}
			}
		}
		metrics: {
			_extra_tags: {
				"source_type": {
					description: "The name of the source type."
					examples: ["vector"]
					required: true
				}
			}
			counter: output._passthrough_counter & {
				tags: _extra_tags
			}
			distribution: output._passthrough_distribution & {
				tags: _extra_tags
			}
			gauge: output._passthrough_gauge & {
				tags: _extra_tags
			}
			histogram: output._passthrough_histogram & {
				tags: _extra_tags
			}
			set: output._passthrough_set & {
				tags: _extra_tags
			}
		}
	}

	telemetry: metrics: {
		component_discarded_events_total:     components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
		protobuf_decode_errors_total:         components.sources.internal_metrics.output.metrics.protobuf_decode_errors_total
	}
}
