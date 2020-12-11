package metadata

components: sinks: aws_sqs: components._aws & {
	title: "Amazon Simple Queue Service (SQS)"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
		service_providers: ["AWS"]
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					default: null
					enum: ["json", "text"]
				}
			}
			request: {
				enabled:                    true
				concurrency:                5
				rate_limit_duration_secs:   1
				rate_limit_num:             5
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               30
			}
			tls: enabled: false
			to: {
				service: services.aws_sqs

				interface: {
					socket: {
						api: {
							title: "Amazon Simple Queue Service API"
							url:   urls.aws_sqs_api
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
		queue_url: {
			description: "The URL of the Amazon SQS queue to which messages are sent."
			required:    true
			warnings: []
			type: string: {
				examples: ["https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"]
			}
		}
		message_group_id: {
			common:      false
			description: "The tag that specifies that a message belongs to a specific message group. Can be applied only to FIFO queues."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["vector", "vector-%Y-%m-%d"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
