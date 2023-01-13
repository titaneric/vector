package metadata

base: components: sinks: elasticsearch: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: """
				Whether or not end-to-end acknowledgements are enabled.

				When enabled for a sink, any source connected to that sink, where the source supports
				end-to-end acknowledgements as well, will wait for events to be acknowledged by the sink
				before acknowledging them at the source.

				Enabling or disabling acknowledgements at the sink level takes precedence over any global
				[`acknowledgements`][global_acks] configuration.

				[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
				"""
			required: false
			type: bool: {}
		}
	}
	api_version: {
		description: "The API version of Elasticsearch."
		required:    false
		type: string: {
			default: "auto"
			enum: {
				auto: "Auto-detect the api version. Will fail if endpoint isn't reachable."
				v6:   "Use the Elasticsearch 6.x API."
				v7:   "Use the Elasticsearch 7.x API."
				v8:   "Use the Elasticsearch 8.x API."
			}
		}
	}
	auth: {
		description: "Authentication strategies."
		required:    false
		type: object: options: {
			access_key_id: {
				description:   "The AWS access key ID."
				relevant_when: "strategy = \"aws\""
				required:      true
				type: string: {}
			}
			assume_role: {
				description:   "The ARN of the role to assume."
				relevant_when: "strategy = \"aws\""
				required:      true
				type: string: {}
			}
			credentials_file: {
				description:   "Path to the credentials file."
				relevant_when: "strategy = \"aws\""
				required:      true
				type: string: {}
			}
			imds: {
				description:   "Configuration for authenticating with AWS through IMDS."
				relevant_when: "strategy = \"aws\""
				required:      false
				type: object: options: {
					connect_timeout_seconds: {
						description: "Connect timeout for IMDS."
						required:    false
						type: uint: {
							default: 1
							unit:    "seconds"
						}
					}
					max_attempts: {
						description: "Number of IMDS retries for fetching tokens and metadata."
						required:    false
						type: uint: default: 4
					}
					read_timeout_seconds: {
						description: "Read timeout for IMDS."
						required:    false
						type: uint: {
							default: 1
							unit:    "seconds"
						}
					}
				}
			}
			load_timeout_secs: {
				description:   "Timeout for successfully loading any credentials, in seconds."
				relevant_when: "strategy = \"aws\""
				required:      false
				type: uint: {}
			}
			password: {
				description:   "Basic authentication password."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: {}
			}
			profile: {
				description:   "The credentials profile to use."
				relevant_when: "strategy = \"aws\""
				required:      false
				type: string: {}
			}
			region: {
				description: """
					The AWS region to send STS requests to.

					If not set, this will default to the configured region
					for the service itself.
					"""
				relevant_when: "strategy = \"aws\""
				required:      false
				type: string: {}
			}
			secret_access_key: {
				description:   "The AWS secret access key."
				relevant_when: "strategy = \"aws\""
				required:      true
				type: string: {}
			}
			strategy: {
				description: "The authentication strategy to use."
				required:    true
				type: string: enum: {
					aws:   "Amazon OpenSearch Service-specific authentication."
					basic: "HTTP Basic Authentication."
				}
			}
			user: {
				description:   "Basic authentication username."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: {}
			}
		}
	}
	aws: {
		description: "Configuration of the region/endpoint to use when interacting with an AWS service."
		required:    false
		type: object: options: {
			endpoint: {
				description: "The API endpoint of the service."
				required:    false
				type: string: {}
			}
			region: {
				description: "The AWS region to use."
				required:    false
				type: string: {}
			}
		}
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type: object: options: {
			max_bytes: {
				description: """
					The maximum size of a batch that will be processed by a sink.

					This is based on the uncompressed size of the batched events, before they are
					serialized / compressed.
					"""
				required: false
				type: uint: {}
			}
			max_events: {
				description: "The maximum size of a batch, in events, before it is flushed."
				required:    false
				type: uint: {}
			}
			timeout_secs: {
				description: "The maximum age of a batch, in seconds, before it is flushed."
				required:    false
				type: float: {}
			}
		}
	}
	bulk: {
		description: "Bulk mode configuration."
		required:    false
		type: object: options: {
			action: {
				description: "The bulk action to use."
				required:    false
				type: string: {}
			}
			index: {
				description: "The name of the index to use."
				required:    false
				type: string: {}
			}
		}
	}
	compression: {
		description: """
			Compression configuration.

			All compression algorithms use the default compression level unless otherwise specified.
			"""
		required: false
		type: string: {
			default: "none"
			enum: {
				gzip: """
					[Gzip][gzip] compression.

					[gzip]: https://www.gzip.org/
					"""
				none: "No compression."
				zlib: """
					[Zlib]][zlib] compression.

					[zlib]: https://zlib.net/
					"""
			}
		}
	}
	data_stream: {
		description: "Data stream mode configuration."
		required:    false
		type: object: options: {
			auto_routing: {
				description: """
					Automatically routes events by deriving the data stream name using specific event fields.

					The format of the data stream name is `<type>-<dataset>-<namespace>`, where each value comes
					from the `data_stream` configuration field of the same name.

					If enabled, the value of the `data_stream.type`, `data_stream.dataset`, and
					`data_stream.namespace` event fields will be used if they are present. Otherwise, the values
					set here in the configuration will be used.
					"""
				required: false
				type: bool: default: true
			}
			dataset: {
				description: "The data stream dataset used to construct the data stream at index time."
				required:    false
				type: string: {
					default: "generic"
					syntax:  "template"
				}
			}
			namespace: {
				description: "The data stream namespace used to construct the data stream at index time."
				required:    false
				type: string: {
					default: "default"
					syntax:  "template"
				}
			}
			sync_fields: {
				description: """
					Automatically adds and syncs the `data_stream.*` event fields if they are missing from the event.

					This ensures that fields match the name of the data stream that is receiving events.
					"""
				required: false
				type: bool: default: true
			}
			type: {
				description: "The data stream type used to construct the data stream at index time."
				required:    false
				type: string: {
					default: "logs"
					syntax:  "template"
				}
			}
		}
	}
	distribution: {
		description: "Options for determining health of an endpoint."
		required:    false
		type: object: options: {
			retry_initial_backoff_secs: {
				description: "Initial timeout, in seconds, between attempts to reactivate endpoints once they become unhealthy."
				required:    false
				type: uint: {}
			}
			retry_max_duration_secs: {
				description: "Maximum timeout, in seconds, between attempts to reactivate endpoints once they become unhealthy."
				required:    false
				type: uint: {}
			}
		}
	}
	doc_type: {
		description: """
			The `doc_type` for your index data.

			This is only relevant for Elasticsearch <= 6.X. If you are using >= 7.0 you do not need to
			set this option since Elasticsearch has removed it.
			"""
		required: false
		type: string: {}
	}
	encoding: {
		description: "Transformations to prepare an event for serialization."
		required:    false
		type: object: options: {
			except_fields: {
				description: "List of fields that will be excluded from the encoded event."
				required:    false
				type: array: items: type: string: {}
			}
			only_fields: {
				description: "List of fields that will be included in the encoded event."
				required:    false
				type: array: items: type: string: {}
			}
			timestamp_format: {
				description: "Format used for timestamp fields."
				required:    false
				type: string: enum: {
					rfc3339: "Represent the timestamp as a RFC 3339 timestamp."
					unix:    "Represent the timestamp as a Unix timestamp."
				}
			}
		}
	}
	endpoint: {
		description: """
			The Elasticsearch endpoint to send logs to.

			This should be the full URL as shown in the example.
			"""
		required: false
		type: string: {}
	}
	endpoints: {
		description: """
			The Elasticsearch endpoints to send logs to.

			Each endpoint should be the full URL as shown in the example.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: {}
		}
	}
	id_key: {
		description: """
			The name of the event key that should map to Elasticsearch’s [`_id` field][es_id].

			By default, the `_id` field is not set, which allows Elasticsearch to set this
			automatically. Setting your own Elasticsearch IDs can [hinder performance][perf_doc].

			[es_id]: https://www.elastic.co/guide/en/elasticsearch/reference/current/mapping-id-field.html
			[perf_doc]: https://www.elastic.co/guide/en/elasticsearch/reference/master/tune-for-indexing-speed.html#_use_auto_generated_ids
			"""
		required: false
		type: string: {}
	}
	metrics: {
		description: "Configuration for the `metric_to_log` transform."
		required:    false
		type: object: options: {
			host_tag: {
				description: """
					Name of the tag in the metric to use for the source host.

					If present, the value of the tag is set on the generated log event in the "host" field,
					where the field key will use the [global `host_key` option][global_log_schema_host_key].

					[global_log_schema_host_key]: https://vector.dev/docs/reference/configuration//global-options#log_schema.host_key
					"""
				required: false
				type: string: examples: ["host", "hostname"]
			}
			metric_tag_values: {
				description: """
					Controls how metric tag values are encoded.

					When set to `single`, only the last non-bare value of tags will be displayed with the
					metric.  When set to `full`, all metric tags will be exposed as separate assignments as
					described by [the `native_json` codec][vector_native_json].
					"""
				required: false
				type: string: {
					default: "single"
					enum: {
						full: "All tags will be exposed as arrays of either string or null values."
						single: """
															Tag values will be exposed as single strings, the same as they were before this config
															option. Tags with multiple values will show the last assigned value, and null values will be
															ignored.
															"""
					}
				}
			}
			timezone: {
				description: """
					The name of the timezone to apply to timestamp conversions that do not contain an explicit
					time zone.

					This overrides the [global `timezone`][global_timezone] option. The time zone name may be
					any name in the [TZ database][tz_database], or `local` to indicate system local time.

					[global_timezone]: https://vector.dev/docs/reference/configuration//global-options#timezone
					[tz_database]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
					"""
				required: false
				type: string: examples: ["local", "America/New_York", "EST5EDT"]
			}
		}
	}
	mode: {
		description: "Indexing mode."
		required:    false
		type: string: {
			default: "bulk"
			enum: {
				bulk: "Ingests documents in bulk, via the bulk API `index` action."
				data_stream: """
					Ingests documents in bulk, via the bulk API `create` action.

					Elasticsearch Data Streams only support the `create` action.
					"""
			}
		}
	}
	pipeline: {
		description: "The name of the pipeline to apply."
		required:    false
		type: string: {}
	}
	query: {
		description: "Custom parameters to add to the query string of each request sent to Elasticsearch."
		required:    false
		type: object: options: "*": {
			description: "A query string parameter."
			required:    true
			type: string: {}
		}
	}
	request: {
		description: "Outbound HTTP request settings."
		required:    false
		type: object: options: {
			adaptive_concurrency: {
				description: """
					Configuration of adaptive concurrency parameters.

					These parameters typically do not require changes from the default, and incorrect values can lead to meta-stable or
					unstable performance and sink behavior. Proceed with caution.
					"""
				required: false
				type: object: options: {
					decrease_ratio: {
						description: """
																The fraction of the current value to set the new concurrency limit when decreasing the limit.

																Valid values are greater than `0` and less than `1`. Smaller values cause the algorithm to scale back rapidly
																when latency increases.

																Note that the new limit is rounded down after applying this ratio.
																"""
						required: false
						type: float: default: 0.9
					}
					ewma_alpha: {
						description: """
																The weighting of new measurements compared to older measurements.

																Valid values are greater than `0` and less than `1`.

																ARC uses an exponentially weighted moving average (EWMA) of past RTT measurements as a reference to compare with
																the current RTT. Smaller values cause this reference to adjust more slowly, which may be useful if a service has
																unusually high response variability.
																"""
						required: false
						type: float: default: 0.4
					}
					rtt_deviation_scale: {
						description: """
																Scale of RTT deviations which are not considered anomalous.

																Valid values are greater than or equal to `0`, and we expect reasonable values to range from `1.0` to `3.0`.

																When calculating the past RTT average, we also compute a secondary “deviation” value that indicates how variable
																those values are. We use that deviation when comparing the past RTT average to the current measurements, so we
																can ignore increases in RTT that are within an expected range. This factor is used to scale up the deviation to
																an appropriate range.  Larger values cause the algorithm to ignore larger increases in the RTT.
																"""
						required: false
						type: float: default: 2.5
					}
				}
			}
			concurrency: {
				description: "Configuration for outbound request concurrency."
				required:    false
				type: {
					string: {
						default: "none"
						enum: {
							adaptive: """
															Concurrency will be managed by Vector's [Adaptive Request Concurrency][arc] feature.

															[arc]: https://vector.dev/docs/about/under-the-hood/networking/arc/
															"""
							none: """
															A fixed concurrency of 1.

															Only one request can be outstanding at any given time.
															"""
						}
					}
					uint: {}
				}
			}
			headers: {
				description: "Additional HTTP headers to add to every HTTP request."
				required:    false
				type: object: options: "*": {
					description: "An HTTP request header."
					required:    true
					type: string: {}
				}
			}
			rate_limit_duration_secs: {
				description: "The time window, in seconds, used for the `rate_limit_num` option."
				required:    false
				type: uint: default: 1
			}
			rate_limit_num: {
				description: "The maximum number of requests allowed within the `rate_limit_duration_secs` time window."
				required:    false
				type: uint: default: 9223372036854775807
			}
			retry_attempts: {
				description: """
					The maximum number of retries to make for failed requests.

					The default, for all intents and purposes, represents an infinite number of retries.
					"""
				required: false
				type: uint: default: 9223372036854775807
			}
			retry_initial_backoff_secs: {
				description: """
					The amount of time to wait before attempting the first retry for a failed request.

					After the first retry has failed, the fibonacci sequence will be used to select future backoffs.
					"""
				required: false
				type: uint: default: 1
			}
			retry_max_duration_secs: {
				description: "The maximum amount of time, in seconds, to wait between retries."
				required:    false
				type: uint: default: 3600
			}
			timeout_secs: {
				description: """
					The maximum time a request can take before being aborted.

					It is highly recommended that you do not lower this value below the service’s internal timeout, as this could
					create orphaned requests, pile on retries, and result in duplicate data downstream.
					"""
				required: false
				type: uint: default: 60
			}
		}
	}
	request_retry_partial: {
		description: """
			Whether or not to retry successful requests containing partial failures.

			To avoid duplicates in Elasticsearch, please use option `id_key`.
			"""
		required: false
		type: bool: default: false
	}
	suppress_type_name: {
		description: """
			Whether or not to send the `type` field to Elasticsearch.

			`type` field was deprecated in Elasticsearch 7.x and removed in Elasticsearch 8.x.

			If enabled, the `doc_type` option will be ignored.

			This option has been deprecated, the `api_version` option should be used instead.
			"""
		required: false
		type: bool: {}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with peer. Prioritized in the order
					they are defined.
					"""
				required: false
				type: array: items: type: string: examples: ["h2"]
			}
			ca_file: {
				description: """
					Absolute path to an additional CA certificate file.

					The certificate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: examples: ["/path/to/certificate_authority.crt"]
			}
			crt_file: {
				description: """
					Absolute path to a certificate file used to identify this server.

					The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
					an inline string in PEM format.

					If this is set, and is not a PKCS#12 archive, `key_file` must also be set.
					"""
				required: false
				type: string: examples: ["/path/to/host_certificate.crt"]
			}
			key_file: {
				description: """
					Absolute path to a private key file used to identify this server.

					The key must be in DER or PEM (PKCS#8) format. Additionally, the key can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: examples: ["/path/to/host_certificate.key"]
			}
			key_pass: {
				description: """
					Passphrase used to unlock the encrypted key file.

					This has no effect unless `key_file` is set.
					"""
				required: false
				type: string: examples: ["${KEY_PASS_ENV_VAR}", "PassWord1"]
			}
			verify_certificate: {
				description: """
					Enables certificate verification.

					If enabled, certificates must be valid in terms of not being expired, as well as being issued by a trusted
					issuer. This verification operates in a hierarchical manner, checking that not only the leaf certificate (the
					certificate presented by the client/server) is valid, but also that the issuer of that certificate is valid, and
					so on until reaching a root certificate.

					Relevant for both incoming and outgoing connections.

					Do NOT set this to `false` unless you understand the risks of not verifying the validity of certificates.
					"""
				required: false
				type: bool: {}
			}
			verify_hostname: {
				description: """
					Enables hostname verification.

					If enabled, the hostname used to connect to the remote host must be present in the TLS certificate presented by
					the remote host, either as the Common Name or as an entry in the Subject Alternative Name extension.

					Only relevant for outgoing connections.

					Do NOT set this to `false` unless you understand the risks of not verifying the remote hostname.
					"""
				required: false
				type: bool: {}
			}
		}
	}
}
