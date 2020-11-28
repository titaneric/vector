package metadata

components: sinks: logdna: {
	title:       "LogDNA"
	description: "[LogDNA](\(urls.logdna)) is a log management system that allows engineering and DevOps to aggregate all system, server, and application logs into one platform. Collect, monitor, store, tail, and search application logs in with one command-line or web interface."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["LogDNA"]
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10490000
				max_events:   null
				timeout_secs: 1
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			request: {
				enabled:                    true
				concurrency:                5
				rate_limit_duration_secs:   1
				rate_limit_num:             5
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               60
			}
			tls: enabled: false
			to: {
				service: {
					name:     "LogDNA"
					thing:    "a \(name) account"
					url:      urls.logdna
					versions: null
				}

				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
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

	configuration: {
		api_key: {
			description: "The Ingestion API key."
			required:    true
			warnings: []
			type: string: {
				examples: ["${LOGDNA_API_KEY}", "ef8d5de700e7989468166c40fc8a0ccd"]
			}
		}
		default_app: {
			common:      false
			description: "The default app that will be set for events that do not contain a `file` or `app` field."
			required:    false
			warnings: []
			type: string: {
				default: "vector"
				examples: ["vector", "myapp"]
			}
		}
		default_env: {
			common:      false
			description: "The default environment that will be set for events that do not contain an `env` field."
			required:    false
			warnings: []
			type: string: {
				default: "production"
				examples: ["staging", "production"]
			}
		}
		endpoint: {
			common:      false
			description: "The endpoint to send logs to."
			required:    false
			type: string: {
				default: "https://logs.logdna.com/logs/ingest"
				examples: ["http://127.0.0.1", "http://example.com"]
			}
		}
		hostname: {
			description: "The hostname that will be attached to each batch of events."
			required:    true
			warnings: []
			type: string: {
				examples: ["${HOSTNAME}", "my-local-machine"]
			}
		}
		ip: {
			common:      false
			description: "The IP address that will be attached to each batch of events."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["0.0.0.0"]
			}
		}
		mac: {
			common:      false
			description: "The mac address that will be attached to each batch of events."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["my-mac-address"]
			}
		}
		tags: {
			common:      false
			description: "The tags that will be attached to each batch of events."
			required:    false
			warnings: []
			type: array: {
				default: null
				items: type: string: examples: ["tag1", "tag2"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
