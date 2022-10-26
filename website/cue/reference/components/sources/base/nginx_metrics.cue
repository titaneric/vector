package metadata

base: components: sources: nginx_metrics: configuration: {
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
	endpoints: {
		description: """
			A list of NGINX instances to scrape.

			Each endpoint must be a valid HTTP/HTTPS URI pointing to an NGINX instance that has the
			`ngx_http_stub_status_module` module enabled.
			"""
		required: true
		type: array: items: type: string: syntax: "literal"
	}
	namespace: {
		description: """
			Overrides the default namespace for the metrics emitted by the source.

			If set to an empty string, no namespace is added to the metrics.

			By default, `nginx` is used.
			"""
		required: false
		type: string: {
			default: "nginx"
			syntax:  "literal"
		}
	}
	scrape_interval_secs: {
		description: "The interval between scrapes, in seconds."
		required:    false
		type: uint: default: 15
	}
	tls: {
		description: "Standard TLS options."
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
