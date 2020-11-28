package metadata

components: sinks: _datadog: {
	description: "[Datadog](\(urls.datadog)) is a monitoring service for cloud-scale applications, providing monitoring of servers, databases, tools, and services, through a SaaS-based data analytics platform."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["Datadog"]
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
			description: "Datadog [API key](https://docs.datadoghq.com/api/?lang=bash#authentication)"
			required:    true
			warnings: []
			type: string: {
				examples: ["${DATADOG_API_KEY_ENV_VAR}", "ef8d5de700e7989468166c40fc8a0ccd"]
			}
		}
		endpoint: {
			common:        false
			description:   "The endpoint to send data to."
			relevant_when: "region is not set"
			required:      false
			type: string: {
				default: null
				examples: ["127.0.0.1:8080", "example.com:12345"]
			}
		}
		region: {
			description:   "The region to send data to."
			required:      false
			relevant_when: "endpoint is not set"
			warnings: []
			type: string: {
				enum: {
					us: "United States"
					eu: "Europe"
				}
			}
		}
	}
}
