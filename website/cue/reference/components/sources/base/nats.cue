package metadata

base: components: sources: nats: configuration: {
	auth: {
		description: "Configuration of the authentication strategy when interacting with NATS."
		required:    false
		type: object: options: {
			credentials_file: {
				description:   "Credentials file configuration."
				relevant_when: "strategy = \"credentials_file\""
				required:      true
				type: object: options: path: {
					description: "Path to credentials file."
					required:    true
					type: string: syntax: "literal"
				}
			}
			nkey: {
				description:   "NKeys configuration."
				relevant_when: "strategy = \"nkey\""
				required:      true
				type: object: options: {
					nkey: {
						description: """
																User.

																Conceptually, this is equivalent to a public key.
																"""
						required: true
						type: string: syntax: "literal"
					}
					seed: {
						description: """
																Seed.

																Conceptually, this is equivalent to a private key.
																"""
						required: true
						type: string: syntax: "literal"
					}
				}
			}
			strategy: {
				required: true
				type: string: enum: {
					credentials_file: """
						Credentials file authentication.
						([documentation](https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/jwt))
						"""
					nkey: """
						NKey authentication.
						([documentation](https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/nkey_auth))
						"""
					token: """
						Token authentication.
						([documentation](https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/tokens))
						"""
					user_password: """
						Username and password authentication.
						([documentation](https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro/username_password))
						"""
				}
			}
			token: {
				description:   "Token configuration."
				relevant_when: "strategy = \"token\""
				required:      true
				type: object: options: value: {
					description: "Token."
					required:    true
					type: string: syntax: "literal"
				}
			}
			user_password: {
				description:   "Username and password configuration."
				relevant_when: "strategy = \"user_password\""
				required:      true
				type: object: options: {
					password: {
						description: "Password."
						required:    true
						type: string: syntax: "literal"
					}
					user: {
						description: "Username."
						required:    true
						type: string: syntax: "literal"
					}
				}
			}
		}
	}
	connection_name: {
		description: "A name assigned to the NATS connection."
		required:    true
		type: string: syntax: "literal"
	}
	decoding: {
		description: "Configuration for building a `Deserializer`."
		required:    false
		type: object: options: codec: {
			required: false
			type: string: {
				default: "bytes"
				enum: {
					bytes:       "Configures the `BytesDeserializer`."
					gelf:        "Configures the `GelfDeserializer`."
					json:        "Configures the `JsonDeserializer`."
					native:      "Configures the `NativeDeserializer`."
					native_json: "Configures the `NativeJsonDeserializer`."
					syslog:      "Configures the `SyslogDeserializer`."
				}
			}
		}
	}
	framing: {
		description: "Configuration for building a `Framer`."
		required:    false
		type: object: options: {
			character_delimited: {
				description:   "Options for the character delimited decoder."
				relevant_when: "method = \"character_delimited\""
				required:      true
				type: object: options: {
					delimiter: {
						description: "The character that delimits byte sequences."
						required:    true
						type: uint: {}
					}
					max_length: {
						description: """
																The maximum length of the byte buffer.

																This length does *not* include the trailing delimiter.
																"""
						required: false
						type: uint: {}
					}
				}
			}
			method: {
				required: false
				type: string: {
					default: "bytes"
					enum: {
						bytes:               "Configures the `BytesDecoder`."
						character_delimited: "Configures the `CharacterDelimitedDecoder`."
						length_delimited:    "Configures the `LengthDelimitedDecoder`."
						newline_delimited:   "Configures the `NewlineDelimitedDecoder`."
						octet_counting:      "Configures the `OctetCountingDecoder`."
					}
				}
			}
			newline_delimited: {
				description:   "Options for the newline delimited decoder."
				relevant_when: "method = \"newline_delimited\""
				required:      false
				type: object: options: max_length: {
					description: """
						The maximum length of the byte buffer.

						This length does *not* include the trailing delimiter.
						"""
					required: false
					type: uint: {}
				}
			}
			octet_counting: {
				description:   "Options for the octet counting decoder."
				relevant_when: "method = \"octet_counting\""
				required:      false
				type: object: options: max_length: {
					description: "The maximum length of the byte buffer."
					required:    false
					type: uint: {}
				}
			}
		}
	}
	queue: {
		description: "NATS Queue Group to join."
		required:    false
		type: string: syntax: "literal"
	}
	subject: {
		description: "The NATS subject to publish messages to."
		required:    true
		type: string: syntax: "template"
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
	url: {
		description: """
			The NATS URL to connect to.

			The URL must take the form of `nats://server:port`.
			"""
		required: true
		type: string: syntax: "literal"
	}
}
