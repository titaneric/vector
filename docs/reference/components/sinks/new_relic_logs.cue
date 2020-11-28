package metadata

components: sinks: new_relic_logs: {
	title:       "New Relic Logs"
	description: "[New Relic][urls.new_relic] is a San Francisco, California-based technology company which develops cloud-based software to help website and application owners track the performances of their services."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["New Relic"]
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    5240000
				max_events:   null
				timeout_secs: 1
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: enabled: false
			}
			request: {
				enabled:                    true
				concurrency:                100
				rate_limit_duration_secs:   1
				rate_limit_num:             100
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               30
			}
			tls: enabled: false
			to: {
				service: {
					name:     "New Relic logs"
					thing:    "a \(name) account"
					url:      urls.new_relic
					versions: null
				}

				interface: {
					socket: {
						api: {
							title: "New Relic  Log API"
							url:   urls.new_relic_log_api
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
		insert_key: {
			common:      true
			description: "Your New Relic insert key (if applicable)."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["xxxx", "${NEW_RELIC_INSERT_KEY}"]
			}
		}
		license_key: {
			common:      true
			description: "Your New Relic license key (if applicable)."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["xxxx", "${NEW_RELIC_LICENSE_KEY}"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
