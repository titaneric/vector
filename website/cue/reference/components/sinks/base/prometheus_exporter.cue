package metadata

base: components: sinks: prometheus_exporter: configuration: {
	acknowledgements: {
		description: "Configuration of acknowledgement behavior."
		required:    false
		type: object: options: enabled: {
			description: "Enables end-to-end acknowledgements."
			required:    false
			type: bool: {}
		}
	}
	address: {
		description: """
			The address to expose for scraping.

			The metrics are exposed at the typical Prometheus exporter path, `/metrics`.
			"""
		required: false
		type: string: {
			default: "0.0.0.0:9598"
			syntax:  "literal"
		}
	}
	auth: {
		description: """
			Configuration of the authentication strategy for HTTP requests.

			HTTP authentication should almost always be used with HTTPS only, as the authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type: object: options: {
			password: {
				description:   "The password to send."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: syntax: "literal"
			}
			strategy: {
				required: true
				type: string: enum: {
					basic: """
						Basic authentication.

						The username and password are concatenated and encoded via base64.
						"""
					bearer: """
						Bearer authentication.

						A bearer token (OAuth2, JWT, etc) is passed as-is.
						"""
				}
			}
			token: {
				description:   "The bearer token to send."
				relevant_when: "strategy = \"bearer\""
				required:      true
				type: string: syntax: "literal"
			}
			user: {
				description:   "The username to send."
				relevant_when: "strategy = \"basic\""
				required:      true
				type: string: syntax: "literal"
			}
		}
	}
	buckets: {
		description: """
			Default buckets to use for aggregating [distribution][dist_metric_docs] metrics into histograms.

			[dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
			"""
		required: false
		type: array: {
			default: [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
			items: type: number: {}
		}
	}
	default_namespace: {
		description: """
			The default namespace for any metrics sent.

			This namespace is only used if a metric has no existing namespace. When a namespace is
			present, it is used as a prefix to the metric name, and separated with an underscore (`_`).

			It should follow the Prometheus [naming conventions][prom_naming_docs].

			[prom_naming_docs]: https://prometheus.io/docs/practices/naming/#metric-names
			"""
		required: false
		type: string: syntax: "literal"
	}
	distributions_as_summaries: {
		description: """
			Whether or not to render [distributions][dist_metric_docs] as an [aggregated histogram][prom_agg_hist_docs] or  [aggregated summary][prom_agg_summ_docs].

			While Vector supports distributions as a lossless way to represent a set of samples for a
			metric, Prometheus clients (the application being scraped, which is this sink) must
			aggregate locally into either an aggregated histogram or aggregated summary.

			[dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
			[prom_agg_hist_docs]: https://prometheus.io/docs/concepts/metric_types/#histogram
			[prom_agg_summ_docs]: https://prometheus.io/docs/concepts/metric_types/#summary
			"""
		required: false
		type: bool: default: false
	}
	flush_period_secs: {
		description: """
			The interval, in seconds, on which metrics are flushed.

			On the flush interval, if a metric has not been seen since the last flush interval, it is
			considered expired and is removed.

			Be sure to configure this value higher than your client’s scrape interval.
			"""
		required: false
		type: uint: {
			default: 60
			unit:    "seconds"
		}
	}
	quantiles: {
		description: """
			Quantiles to use for aggregating [distribution][dist_metric_docs] metrics into a summary.

			[dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
			"""
		required: false
		type: array: {
			default: [0.5, 0.75, 0.9, 0.95, 0.99]
			items: type: number: {}
		}
	}
	suppress_timestamp: {
		description: """
			Suppresses timestamps on the Prometheus output.

			This can sometimes be useful when the source of metrics leads to their timestamps being too
			far in the past for Prometheus to allow them, such as when aggregating metrics over long
			time periods, or when replaying old metrics from a disk buffer.
			"""
		required: false
		type: bool: default: false
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type: object: options: {
			alpn_protocols: {
				description: """
					Sets the list of supported ALPN protocols.

					Declare the supported ALPN protocols, which are used during negotiation with peer. Prioritized in the order
					they are defined.
					"""
				required: false
				type: array: items: type: string: syntax: "literal"
			}
			ca_file: {
				description: """
					Absolute path to an additional CA certificate file.

					The certficate must be in the DER or PEM (X.509) format. Additionally, the certificate can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: syntax: "literal"
			}
			crt_file: {
				description: """
					Absolute path to a certificate file used to identify this server.

					The certificate must be in DER, PEM (X.509), or PKCS#12 format. Additionally, the certificate can be provided as
					an inline string in PEM format.

					If this is set, and is not a PKCS#12 archive, `key_file` must also be set.
					"""
				required: false
				type: string: syntax: "literal"
			}
			enabled: {
				description: """
					Whether or not to require TLS for incoming/outgoing connections.

					When enabled and used for incoming connections, an identity certificate is also required. See `tls.crt_file` for
					more information.
					"""
				required: false
				type: bool: {}
			}
			key_file: {
				description: """
					Absolute path to a private key file used to identify this server.

					The key must be in DER or PEM (PKCS#8) format. Additionally, the key can be provided as an inline string in PEM format.
					"""
				required: false
				type: string: syntax: "literal"
			}
			key_pass: {
				description: """
					Passphrase used to unlock the encrypted key file.

					This has no effect unless `key_file` is set.
					"""
				required: false
				type: string: syntax: "literal"
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
