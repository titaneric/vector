package metadata

components: sources: http: {
	_port: 80

	title:             "HTTP"
	long_description:  ""
	short_description: "Receive logs through the HTTP protocol"

	classes: {
		commonly_used: false
		deployment_roles: ["aggregator", "sidecar"]
		egress_method: "batch"
		function:      "receive"
	}

	features: {
		checkpoint: enabled: false
		multiline: enabled:  false
		tls: {
			enabled:                true
			can_enable:             false
			can_verify_certificate: true
			enabled_default:        false
		}
	}

	statuses: {
		delivery:    "at_least_once"
		development: "beta"
	}

	support: {
		platforms: {
			docker: ports: [_port]
			triples: {
				"aarch64-unknown-linux-gnu":  true
				"aarch64-unknown-linux-musl": true
				"x86_64-apple-darwin":        true
				"x86_64-pc-windows-msv":      true
				"x86_64-unknown-linux-gnu":   true
				"x86_64-unknown-linux-musl":  true
			}
		}

		requirements: [
			#"""
				This component exposes a configured port. You must ensure your network
				allows inbound access to this port if you want to accept requests from
				remote sources.
				"""#,
		]

		warnings: []
		notices: []
	}

	configuration: {
		address: {
			description: "The address to listen for connections on"
			required:    true
			type: string: examples: ["0.0.0.0:\(_port)", "localhost:\(_port)"]
		}
		encoding: {
			common:      true
			description: "The expected encoding of received data. Note that for `json` and `ndjson` encodings, the fields of the JSON objects are output as separate fields."
			required:    false
			type: string: {
				default: "text"
				enum: {
					text:   "Newline-delimited text, with each line forming a message."
					ndjson: "Newline-delimited JSON objects, where each line must contain a JSON object."
					json:   "Array of JSON objects, which must be a JSON array containing JSON objects."
				}
			}
		}
		headers: {
			common:      false
			description: "A list of HTTP headers to include in the log event. These will override any values included in the JSON payload with conflicting names. An empty string will be inserted into the log event if the corresponding HTTP header was missing."
			required:    false
			type: array: {
				default: null
				items: type: string: examples: ["User-Agent", "X-My-Custom-Header"]
			}
		}
	}

	output: logs: {
		text: {
			description: "An individual line from a `text/plain` request"
			fields: {
				message: {
					description:   "The raw line line from the incoming payload."
					relevant_when: "`encoding` == \"text\""
					required:      true
					type: string: examples: ["Hello world"]
				}
				timestamp: fields._current_timestamp
			}
		}
		structured: {
			description: "An individual line from a `application/json` request"
			fields: {
				"*": {
					common:        false
					description:   "Any field contained in your JSON payload"
					relevant_when: "`encoding` != \"text\""
					required:      false
					type: "*": {}
				}
				timestamp: fields._current_timestamp
			}
		}
	}

	examples: [
		{
			_line:       "Hello world"
			_user_agent: "my-service/v2.1"
			title:       "text/plain"
			configuration: {
				address:  "0.0.0.0:\(_port)"
				encoding: "text"
				headers: ["User-Agent"]
			}
			input: """
             ```http
             Content-Type: text/plain
             User-Agent: \( _user_agent )
             X-Forwarded-For: \( _values.local_host )

             \( _line )
             ```
             """
			output: [{
				log: {
					host:         _values.local_host
					message:      _line
					timestamp:    _values.current_timestamp
					"User-Agent": _user_agent
				}
			}]
		},
		{
			_line:       "{\"key\": \"val\"}"
			_user_agent: "my-service/v2.1"
			title:       "application/json"
			configuration: {
				address:  "0.0.0.0:\(_port)"
				encoding: "json"
				headers: ["User-Agent"]
			}
			input: """
             ```http
             Content-Type: application/json
             User-Agent: \( _user_agent )
             X-Forwarded-For: \( _values.local_host )

             \( _line )
             ```
             """
			output: [{
				log: {
					host:         _values.local_host
					key:          "val"
					timestamp:    _values.current_timestamp
					"User-Agent": _user_agent
				}
			}]
		},
	]
}
