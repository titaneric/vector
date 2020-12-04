package metadata

components: {
	// `#Classes` represent various `#Components` classifications.
	#Classes: {
		_args: kind: string
		let Args = _args

		// `commonly_used` specifies if the component is commonly used or not.
		// Setting this to `true` will surface the component from other,
		// less commonly used, components.
		commonly_used: bool

		if Args.kind == "source" || Args.kind == "sink" {
			delivery: #DeliveryStatus
		}

		if Args.kind == "source" {
			// `deployment_roles` clarify when the component should be used under
			// different deployment contexts.
			deployment_roles: [...#DeploymentRole]
		}
		development: #DevelopmentStatus

		// `egress_method` documents how the component outputs events.
		egress_method: #EgressMethod

		if Args.kind == "sink" {
			// `service_providers` specify the service providers that support
			// and host this service. This helps users find relevant sinks.
			//
			// For example, "AWS" is a service provider for many services, and
			// a user on AWS can use this to filter for AWS supported
			// components.
			service_providers: [string, ...string] | *[]
		}
	}

	#Component: {
		// `kind` specified the component kind. This is set automatically.
		kind: #ComponentKind
		let Kind = kind

		installation?: {
			platform_name: string | null
		}

		configuration: #Schema

		// `description` describes the components with a single paragraph.
		// It is used for SEO purposes and should be full of relevant keywords.
		description?: =~"[.]$"

		env_vars: #EnvVars

		// `alias` is used to register a component's former name when it
		// undergoes a name change.
		alias?: !=""

		// `type` is the component identifier. This is set automatically.
		type: string

		// `classes` represent the various classifications for this component
		classes: #Classes & {_args: kind: Kind}

		// `examples` demonstrates various ways to use the component using an
		// input, output, and example configuration.
		#ExampleConfig: close({
			title:    string
			context?: string
			"configuration": {
				for k, v in configuration {
					"\( k )"?: _ | *null
				}
			}

			if Kind == "source" {
				input: string
			}

			if Kind != "source" {
				input: #Event | [#Event, ...#Event]
			}

			if Kind == "sink" {
				output: string
			}

			if Kind != "sink" {
				output: #Event | [#Event, ...#Event] | null
			}

			notes?: string
		})

		examples?: [#ExampleConfig, ...#ExampleConfig]

		// `features` describes the various supported features of the component.
		// Setting these helps to reduce boilerplate.
		//
		// For example, the `tls` feature will automatically add the appropriate
		// `tls` options and `how_it_works` sections.
		features: #Features & {_args: {egress_method: classes.egress_method, kind: Kind}}

		// `how_it_works` contain sections that further describe the component's
		// behavior. This is like a mini-manual for the component and should help
		// answer any obvious questions the user might have. Options can link
		// to these sections for deeper explanations of behavior.
		how_it_works: #HowItWorks

		if Kind != "source" {
			input: #Input
		}

		if Kind != "sink" {
			// `output` documents output of the component. This is very important
			// as it communicate which events and fields are emitted.
			output: #Output
		}

		// `support` communicates the varying levels of support of the component.
		support: #Support & {_args: kind: Kind}

		// `title` is the human friendly title for the component.
		//
		// For example, the `http` sink has a `HTTP` title.
		title: string

		// Telemetry produced by the component
		telemetry: metrics: #MetricOutput
	}

	// `#ComponentKind` represent the kind of component.
	#ComponentKind: "sink" | "source" | "transform"

	#Components: [Type=string]: #Component & {
		type: Type
	}

	// `#EgressMethod` specified how a component outputs events.
	//
	// * `batch` - one or more events at a time
	// * `dynamic` - can switch between batch and stream based on configuration.
	// * `expose` - exposes data, ex: prometheus_exporter sink
	// * `stream` - one event at a time
	#EgressMethod: "batch" | "dynamic" | "expose" | "stream"

	#EnvVars: #Schema & {[Type=string]: {
		common:   true
		required: false
	}}

	#Features: {
		_args: {
			egress_method: string
			kind:          string
		}
		let Args = _args

		if Args.kind == "source" {
			collect?:  #FeaturesCollect
			generate?: #FeaturesGenerate
			multiline: #FeaturesMultiline
			receive?:  #FeaturesReceive
		}

		if Args.kind == "transform" {
			convert?:  #FeaturesConvert
			enrich?:   #FeaturesEnrich
			filter?:   #FeaturesFilter
			parse?:    #FeaturesParse
			program?:  #FeaturesProgram
			reduce?:   #FeaturesReduce
			route?:    #FeaturesRoute
			sanitize?: #FeaturesSanitize
			shape?:    #FeaturesShape
		}

		if Args.kind == "sink" {
			// `buffer` describes how the component buffers data.
			buffer: close({
				enabled: bool | string
			})

			// `healtcheck` notes if a component offers a healthcheck on boot.
			healthcheck: close({
				enabled: bool
			})

			exposes?: #FeaturesExpose
			send?:    #FeaturesSend & {_args: Args}
		}

		descriptions: [Name=string]: string
	}

	#FeaturesCollect: {
		checkpoint: close({
			enabled: bool
		})

		from?: {
			service:    #Service
			interface?: #Interface
		}

		tls?: #FeaturesTLS & {_args: {mode: "connect"}}
	}

	#FeaturesConvert: {
	}

	#FeaturesEnrich: {
		from: service: close({
			name:     string
			url:      string
			versions: string | null
		})
	}

	#FeaturesExpose: {
		for: {
			service:    #Service
			interface?: #Interface
		}
	}

	#FeaturesFilter: {
	}

	#FeaturesGenerate: {
	}

	#FeaturesKeepalive: {
		enabled: bool
	}

	#FeaturesMultiline: {
		enabled: bool
	}

	#FeaturesParse: {
		format: close({
			name:     string
			url:      string | null
			versions: string | null
		})
	}

	#FeaturesProgram: {
		runtime: #Runtime
	}

	#FeaturesReceive: {
		from?: {
			service:    #Service
			interface?: #Interface
		}

		keepalive?: #FeaturesKeepalive

		tls: #FeaturesTLS & {_args: {mode: "accept"}}
	}

	#FeaturesReduce: {
	}

	#FeaturesRoute: {
	}

	#FeaturesSanitize: {
	}

	#FeaturesShape: {
	}

	#FeaturesSend: {
		_args: {
			egress_method: string
			kind:          string
		}
		let Args = _args

		if Args.egress_method == "batch" || Args.egress_method == "dynamic" {
			// `batch` describes how the component batches data. This is only
			// relevant if a component has an `egress_method` of "batch".
			batch: close({
				enabled:      bool
				common:       bool
				max_bytes:    uint | null
				max_events:   uint | null
				timeout_secs: uint16 | null
			})
		}

		// `compression` describes how the component compresses data.
		compression: {
			enabled: bool

			if enabled == true {
				default: #CompressionAlgorithm
				algorithms: [#CompressionAlgorithm, ...#CompressionAlgorithm]
				levels: [#CompressionLevel, ...#CompressionLevel]
			}
		}

		// `encoding` describes how the component encodes data.
		encoding: {
			enabled: bool

			if enabled {
				codec: {
					enabled: bool

					if enabled {
						default: #EncodingCodec | null
						enum:    [#EncodingCodec, ...#EncodingCodec] | null
					}
				}
			}
		}

		keepalive?: #FeaturesKeepalive

		// `request` describes how the component issues and manages external
		// requests.
		request: {
			enabled: bool

			if enabled {
				adaptive_concurrency:       bool | *true
				concurrency:                uint8 | *5
				rate_limit_duration_secs:   uint8
				rate_limit_num:             uint16
				retry_initial_backoff_secs: uint8
				retry_max_duration_secs:    uint8
				timeout_secs:               uint8
			}
		}

		// `tls` describes if the component secures network communication
		// via TLS.
		tls: #FeaturesTLS & {_args: {mode: "connect"}}

		to?: {
			service:    #Service
			interface?: #Interface
		}
	}

	#FeaturesTLS: {
		_args: {
			mode: "accept" | "connect"
		}
		let Args = _args
		enabled: bool

		if enabled {
			can_enable:             bool
			can_verify_certificate: bool
			if Args.mode == "connect" {
				can_verify_hostname: bool
			}
			enabled_default: bool
		}
	}

	#Input: {
		logs:    bool
		metrics: #MetricInput | null
	}

	#LogOutput: [Name=string]: close({
		description: string
		name:        Name
		fields:      #Schema
	})

	#MetricInput: {
		counter:      bool
		distribution: bool
		gauge:        bool
		histogram:    bool
		summary:      bool
		set:          bool
	}

	#MetricOutput: [Name=string]: close({
		description:       string
		relevant_when?:    string
		tags:              #MetricTags
		name:              Name
		type:              #MetricType
		default_namespace: string
	})

	#Output: {
		logs?:    #LogOutput
		metrics?: #MetricOutput
	}

	#Runtime: {
		name:    string
		url:     string
		version: string | null
	}

	#Support: {
		_args: kind: string

		// `requirements` describes any external requirements that the component
		// needs to function properly.
		//
		// For example, the `journald` source requires the presence of the
		// `journalctl` binary.
		requirements: [...string] | null // Allow for empty list

		// `targets` describes which targets this component is available on.
		targets: #TargetTriples

		// `warnings` describes any warnings the user should know about the
		// component.
		//
		// For example, the `grok_parser` might offer a performance warning
		// since the `regex_parser` and other transforms are faster.
		warnings: [...string] | null // Allow for empty list

		// `notices` communicates useful information to the user that is neither
		// a requirement or a warning.
		//
		// For example, the `lua` transform offers a Lua version notice that
		// communicate which version of Lua is embedded.
		notices: [...string] | null // Allow for empty list
	}

	sources:    #Components
	transforms: #Components
	sinks:      #Components

	{[Kind=string]: [Name=string]: {
		kind: string
		let Kind = kind

		configuration: {
			_conditions: {
				examples: [
					{
						type:                           "check_fields"
						"message.eq":                   "foo"
						"message.not_eq":               "foo"
						"message.exists":               true
						"message.not_exists":           true
						"message.contains":             "foo"
						"message.not_contains":         "foo"
						"message.ends_with":            "foo"
						"message.not_ends_with":        "foo"
						"message.ip_cidr_contains":     "10.0.0.0/8"
						"message.not_ip_cidr_contains": "10.0.0.0/8"
						"message.regex":                " (any|of|these|five|words) "
						"message.not_regex":            " (any|of|these|five|words) "
						"message.starts_with":          "foo"
						"message.not_starts_with":      "foo"
					},
				]
				options: {
					type: {
						common:      true
						description: "The type of the condition to execute."
						required:    false
						warnings: []
						type: string: {
							default: "check_fields"
							enum: {
								check_fields: "Allows you to check individual fields against a list of conditions."
								is_log:       "Returns true if the event is a log."
								is_metric:    "Returns true if the event is a metric."
							}
						}
					}
					"*.eq": {
						common:      true
						description: "Check whether a field's contents exactly matches the value specified, case sensitive. This may be a single string or a list of strings, in which case this evaluates to true if any of the list matches."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["foo"]
						}
					}
					"*.exists": {
						common:      false
						description: "Check whether a field exists or does not exist, depending on the provided value being `true` or `false` respectively."
						required:    false
						warnings: []
						type: bool: default: null
					}
					"*.not_*": {
						common:      false
						description: "Allow you to negate any condition listed here."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: []
						}
					}
					"*.contains": {
						common:      true
						description: "Checks whether a string field contains a string argument, case sensitive. This may be a single string or a list of strings, in which case this evaluates to true if any of the list matches."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["foo"]
						}
					}
					"*.ends_with": {
						common:      true
						description: "Checks whether a string field ends with a string argument, case sensitive. This may be a single string or a list of strings, in which case this evaluates to true if any of the list matches."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["suffix"]
						}
					}
					"*.ip_cidr_contains": {
						common:      false
						description: "Checks whether an IP field is contained within a given [IP CIDR](\(urls.cidr)) (works with IPv4 and IPv6). This may be a single string or a list of strings, in which case this evaluates to true if the IP field is contained within any of the CIDRs in the list."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["10.0.0.0/8", "2000::/10", "192.168.0.0/16"]
						}
					}
					"*.regex": {
						common:      true
						description: "Checks whether a string field matches a [regular expression](\(urls.regex)). Vector uses the [documented Rust Regex syntax](\(urls.rust_regex_syntax)). Note that this condition is considerably more expensive than a regular string match (such as `starts_with` or `contains`) so the use of those conditions are preferred where possible."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: [" (any|of|these|five|words) "]
						}
					}
					"*.starts_with": {
						common:      true
						description: "Checks whether a string field starts with a string argument, case sensitive. This may be a single string or a list of strings, in which case this evaluates to true if any of the list matches."
						required:    false
						warnings: []
						type: string: {
							default: null
							examples: ["prefix"]
						}
					}
				}
			}

			_tls_accept: {
				_args: {
					can_enable:             bool
					can_verify_certificate: bool | *true
					enabled_default:        bool
				}
				let Args = _args

				common:      false
				description: "Configures the TLS options for incoming connections."
				required:    false
				type: object: options: {
					if Args.can_enable {
						enabled: {
							common:      false
							description: "Require TLS for incoming connections. If this is set, an identity certificate is also required."
							required:    false
							type: bool: default: Args.enabled_default
						}
					}

					ca_file: {
						common:      false
						description: "Absolute path to an additional CA certificate file, in DER or PEM format (X.509), or an in-line CA certificate in PEM format."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/certificate_authority.crt"]
						}
					}
					crt_file: {
						common:      false
						description: "Absolute path to a certificate file used to identify this server, in DER or PEM format (X.509) or PKCS#12, or an in-line certificate in PEM format. If this is set, and is not a PKCS#12 archive, `key_file` must also be set. This is required if `enabled` is set to `true`."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.crt"]
						}
					}
					key_file: {
						common:      false
						description: "Absolute path to a private key file used to identify this server, in DER or PEM format (PKCS#8), or an in-line private key in PEM format."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.key"]
						}
					}
					key_pass: {
						common:      false
						description: "Pass phrase used to unlock the encrypted key file. This has no effect unless `key_file` is set."
						required:    false
						type: string: {
							default: null
							examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
						}
					}

					if Args.can_verify_certificate {
						verify_certificate: {
							common:      false
							description: "If `true`, Vector will require a TLS certificate from the connecting host and terminate the connection if the certificate is not valid. If `false` (the default), Vector will not request a certificate from the client."
							required:    false
							type: bool: default: false
						}
					}
				}
			}

			_tls_connect: {
				_args: {
					can_enable:             bool
					can_verify_certificate: bool | *true
					can_verify_hostname:    bool | *false
					enabled_default:        bool
				}
				let Args = _args

				common:      false
				description: "Configures the TLS options for incoming connections."
				required:    false
				type: object: options: {
					if Args.can_enable {
						enabled: {
							common:      true
							description: "Enable TLS during connections to the remote."
							required:    false
							type: bool: default: Args.enabled_default
						}
					}

					ca_file: {
						common:      false
						description: "Absolute path to an additional CA certificate file, in DER or PEM format (X.509), or an inline CA certificate in PEM format."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/certificate_authority.crt"]
						}
					}
					crt_file: {
						common:      true
						description: "Absolute path to a certificate file used to identify this connection, in DER or PEM format (X.509) or PKCS#12, or an inline certificate in PEM format. If this is set and is not a PKCS#12 archive, `key_file` must also be set."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.crt"]
						}
					}
					key_file: {
						common:      true
						description: "Absolute path to a private key file used to identify this connection, in DER or PEM format (PKCS#8), or an inline private key in PEM format. If this is set, `crt_file` must also be set."
						required:    false
						type: string: {
							default: null
							examples: ["/path/to/host_certificate.key"]
						}
					}
					key_pass: {
						common:      false
						description: "Pass phrase used to unlock the encrypted key file. This has no effect unless `key_file` is set."
						required:    false
						type: string: {
							default: null
							examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
						}
					}

					if Args.can_verify_certificate {
						verify_certificate: {
							common:      false
							description: "If `true` (the default), Vector will validate the TLS certificate of the remote host."
							required:    false
							type: bool: default: true
						}
					}

					if Args.can_verify_hostname {
						verify_hostname: {
							common:      false
							description: "If `true` (the default), Vector will validate the configured remote host name against the remote host's TLS certificate. Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname."
							required:    false
							type: bool: default: true
						}
					}
				}
			}

			_http_auth: {
				_args: {
					password_example: string
					username_example: string
				}
				let Args = _args

				common:      false
				description: "Configures the authentication strategy."
				required:    false
				type: object: options: {
					password: {
						description: "The basic authentication password."
						required:    true
						warnings: []
						type: string: {
							examples: [Args.password_example, "password"]
						}
					}
					strategy: {
						description: "The authentication strategy to use."
						required:    true
						warnings: []
						type: string: {
							enum: {
								basic:  "The [basic authentication strategy](\(urls.basic_auth))."
								bearer: "The bearer token authentication strategy."
							}
						}
					}
					token: {
						description: "The token to use for bearer authentication"
						required:    true
						warnings: []
						type: string: {
							examples: ["${API_TOKEN}", "xyz123"]
						}
					}
					user: {
						description: "The basic authentication user name."
						required:    true
						warnings: []
						type: string: {
							examples: [Args.username_example, "username"]
						}
					}
				}
			}

			_http_basic_auth: {
				common:      false
				description: "Options for HTTP Basic Authentication."
				required:    false
				warnings: []
				type: object: {
					examples: []
					options: {
						username: {
							description: "The basic authentication user name."
							required:    true
							warnings: []
							type: string: {
								examples: ["${HTTP_USERNAME}", "username"]
							}
						}
						password: {
							description: "The basic authentication password."
							required:    true
							warnings: []
							type: string: {
								examples: ["${HTTP_PASSWORD}", "password"]
							}
						}
					}
				}
			}

			_types: {
				common:      true
				description: "Key/value pairs representing mapped log field names and types. This is used to coerce log fields into their proper types."
				required:    false
				warnings: []
				type: object: {
					examples: [
						{
							status:            "int"
							duration:          "float"
							success:           "bool"
							timestamp_iso8601: "timestamp|%F"
							timestamp_custom:  "timestamp|%a %b %e %T %Y"
							parent: {"child": "int"}
						},
					]
					options: {}
				}
			}

			if Kind != "source" {
				inputs: {
					description: "A list of upstream [source](\(urls.vector_sources)) or [transform](\(urls.vector_transforms)) IDs. See [configuration](\(urls.vector_configuration)) for more info."
					required:    true
					sort:        -1
					type: array: items: type: string: examples: ["my-source-or-transform-id"]
				}
			}

			"type": {
				description: "The component type. This is a required field for all components and tells Vector which component to use."
				required:    true
				sort:        -2
				"type": string: enum:
					"\(Name)": "The type of this component."
			}
		}

		features: {
			descriptions: {
				if features.buffer != _|_ {
					if features.buffer.enabled == true {
						buffer: "Buffers data in-memory or on-disk for performance and durability."
					}
				}

				if features.collect != _|_ {
					if features.collect.from != _|_ {
						collect_context: "Enriches data with useful \(features.collect.from.service.name) context."
					}

					if features.collect.checkpoint.enabled != _|_ {
						checkpoint: "Efficiently collects data and checkpoints read positions to ensure data is not lost between restarts."
					}

					if features.collect.tls.enabled != _|_ {
						tls_collect: "Securely collects data via Transport Layer Security (TLS)."
					}
				}

				if features.multiline != _|_ {
					if features.multiline.enabled == true {
						multiline: "Merges multi-line logs into one event."
					}
				}

				if features.receive != _|_ {
					if features.receive.from != _|_ {
						receive_context: "Enriches data with useful \(features.receive.from.service.name) context."
					}

					if features.receive.keepalive.enabled != _|_ {
						keepalive: "Supports TCP keepalive for efficient resource use and reliability."
					}

					if features.receive.tls.enabled != _|_ {
						tls_receive: "Securely receives data via Transport Layer Security (TLS)."
					}
				}

				if features.send != _|_ {
					if features.send.batch != _|_ {
						if features.send.batch.enabled {
							batch: "Batches data to maximize throughput."
						}
					}

					if features.send.compression.enabled != _|_ {
						compress: "Compresses data to optimize bandwidth."
					}

					if features.send.keepalive.enabled != _|_ {
						keepalive: "Supports TCP keepalive for efficient resource use and reliability."
					}

					if features.send.request.enabled != _|_ {
						request: "Automatically retries failed requests, with backoff."
					}

					if features.send.tls.enabled != _|_ {
						tls_send: "Securely transmits data via Transport Layer Security (TLS)."
					}
				}
			}
		}

		if Kind == "source" || Kind == "transform" {
			output: {
				_passthrough_counter: {
					description: data_model.schema.metric.type.object.options.counter.description
					tags: {
						"*": {
							description: "Any tags present on the metric."
							examples: [_values.local_host]
							required: false
						}
					}
					type:              "counter"
					default_namespace: "vector"
				}

				_passthrough_distribution: {
					description: data_model.schema.metric.type.object.options.distribution.description
					tags: {
						"*": {
							description: "Any tags present on the metric."
							examples: [_values.local_host]
							required: false
						}
					}
					type:              "distribution"
					default_namespace: "vector"
				}

				_passthrough_gauge: {
					description: data_model.schema.metric.type.object.options.gauge.description
					tags: {
						"*": {
							description: "Any tags present on the metric."
							examples: [_values.local_host]
							required: false
						}
					}
					type:              "gauge"
					default_namespace: "vector"
				}

				_passthrough_histogram: {
					description: data_model.schema.metric.type.object.options.histogram.description
					tags: {
						"*": {
							description: "Any tags present on the metric."
							examples: [_values.local_host]
							required: false
						}
					}
					type:              "gauge"
					default_namespace: "vector"
				}

				_passthrough_set: {
					description: data_model.schema.metric.type.object.options.set.description
					tags: {
						"*": {
							description: "Any tags present on the metric."
							examples: [_values.local_host]
							required: false
						}
					}
					type:              "gauge"
					default_namespace: "vector"
				}

				_passthrough_summary: {
					description: data_model.schema.metric.type.object.options.summary.description
					tags: {
						"*": {
							description: "Any tags present on the metric."
							examples: [_values.local_host]
							required: false
						}
					}
					type:              "gauge"
					default_namespace: "vector"
				}
			}
		}

		telemetry: metrics: {
			// Default metrics for each component
			processed_events_total: components.sources.internal_metrics.output.metrics.processed_events_total
			processed_bytes_total:  components.sources.internal_metrics.output.metrics.processed_bytes_total
		}
	}}
}
