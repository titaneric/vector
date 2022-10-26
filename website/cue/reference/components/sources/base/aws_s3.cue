package metadata

base: components: sources: aws_s3: configuration: {
	acknowledgements: {
		description: "Configuration of acknowledgement behavior."
		required:    false
		type: object: {
			default: enabled: null
			options: enabled: {
				description: "Enables end-to-end acknowledgements."
				required:    false
				type: bool: {}
			}
		}
	}
	assume_role: {
		description: """
			The ARN of an [IAM role][iam_role] to assume at startup.

			[iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
			"""
		required: false
		type: string: syntax: "literal"
	}
	auth: {
		description: "Configuration of the authentication strategy for interacting with AWS services."
		required:    false
		type: object: {
			default: load_timeout_secs: null
			options: {
				access_key_id: {
					description: "The AWS access key ID."
					required:    true
					type: string: syntax: "literal"
				}
				assume_role: {
					description: "The ARN of the role to assume."
					required:    true
					type: string: syntax: "literal"
				}
				credentials_file: {
					description: "Path to the credentials file."
					required:    true
					type: string: syntax: "literal"
				}
				load_timeout_secs: {
					description: "Timeout for successfully loading any credentials, in seconds."
					required:    false
					type: uint: {}
				}
				profile: {
					description: "The credentials profile to use."
					required:    false
					type: string: syntax: "literal"
				}
				region: {
					description: """
						The AWS region to send STS requests to.

						If not set, this will default to the configured region
						for the service itself.
						"""
					required: false
					type: string: syntax: "literal"
				}
				secret_access_key: {
					description: "The AWS secret access key."
					required:    true
					type: string: syntax: "literal"
				}
			}
		}
	}
	compression: {
		description: "The compression scheme used for decompressing objects retrieved from S3."
		required:    false
		type: string: {
			default: "auto"
			enum: {
				auto: """
					Automatically attempt to determine the compression scheme.

					Vector will try to determine the compression scheme of the object from its: `Content-Encoding` and
					`Content-Type` metadata, as well as the key suffix (e.g. `.gz`).

					It will fallback to 'none' if the compression scheme cannot be determined.
					"""
				gzip: "GZIP."
				none: "Uncompressed."
				zstd: "ZSTD."
			}
		}
	}
	endpoint: {
		description: "The API endpoint of the service."
		required:    false
		type: string: syntax: "literal"
	}
	multiline: {
		description: """
			Multiline aggregation configuration.

			If not specified, multiline aggregation is disabled.
			"""
		required: false
		type: object: options: {
			condition_pattern: {
				description: """
					Regular expression pattern that is used to determine whether or not more lines should be read.

					This setting must be configured in conjunction with `mode`.
					"""
				required: true
				type: string: syntax: "literal"
			}
			mode: {
				description: """
					Aggregation mode.

					This setting must be configured in conjunction with `condition_pattern`.
					"""
				required: true
				type: string: enum: {
					continue_past: """
						All consecutive lines matching this pattern, plus one additional line, are included in the group.

						This is useful in cases where a log message ends with a continuation marker, such as a backslash, indicating
						that the following line is part of the same message.
						"""
					continue_through: """
						All consecutive lines matching this pattern are included in the group.

						The first line (the line that matched the start pattern) does not need to match the `ContinueThrough` pattern.

						This is useful in cases such as a Java stack trace, where some indicator in the line (such as leading
						whitespace) indicates that it is an extension of the proceeding line.
						"""
					halt_before: """
						All consecutive lines not matching this pattern are included in the group.

						This is useful where a log line contains a marker indicating that it begins a new message.
						"""
					halt_with: """
						All consecutive lines, up to and including the first line matching this pattern, are included in the group.

						This is useful where a log line ends with a termination marker, such as a semicolon.
						"""
				}
			}
			start_pattern: {
				description: "Regular expression pattern that is used to match the start of a new message."
				required:    true
				type: string: syntax: "literal"
			}
			timeout_ms: {
				description: """
					The maximum amount of time to wait for the next additional line, in milliseconds.

					Once this timeout is reached, the buffered message is guaranteed to be flushed, even if incomplete.
					"""
				required: true
				type: uint: {}
			}
		}
	}
	region: {
		description: "The AWS region to use."
		required:    false
		type: string: syntax: "literal"
	}
	sqs: {
		description: """
			Configuration options for SQS.

			Only relevant when `strategy = "sqs"`.
			"""
		required: false
		type: object: options: {
			client_concurrency: {
				description: """
					Number of concurrent tasks to create for polling the queue for messages.

					Defaults to the number of available CPUs on the system.

					Should not typically need to be changed, but it can sometimes be beneficial to raise this value when there is a
					high rate of messages being pushed into the queue and the objects being fetched are small. In these cases,
					Vector may not fully utilize system resources without fetching more messages per second, as the SQS message
					consumption rate affects the S3 object retrieval rate.
					"""
				required: false
				type: uint: default: 24
			}
			delete_message: {
				description: """
					Whether to delete the message once Vector processes it.

					It can be useful to set this to `false` to debug or during initial Vector setup.
					"""
				required: false
				type: bool: default: true
			}
			poll_secs: {
				description: """
					How long to wait while polling the queue for new messages, in seconds.

					Generally should not be changed unless instructed to do so, as if messages are available, they will always be
					consumed, regardless of the value of `poll_secs`.
					"""
				required: false
				type: uint: default: 15
			}
			queue_url: {
				description: "The URL of the SQS queue to poll for bucket notifications."
				required:    true
				type: string: syntax: "literal"
			}
			tls_options: {
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
			visibility_timeout_secs: {
				description: """
					The visibility timeout to use for messages, in secords.

					This controls how long a message is left unavailable after Vector receives it. If Vector receives a message, and
					takes longer than `visibility_timeout_secs` to process and delete the message from the queue, it will be made reavailable for another consumer.

					This can happen if, for example, if Vector crashes between consuming a message and deleting it.
					"""
				required: false
				type: uint: default: 300
			}
		}
	}
	strategy: {
		description: "The strategy to use to consume objects from S3."
		required:    false
		type: string: {
			default: "sqs"
			enum: sqs: """
				Consumes objects by processing bucket notification events sent to an [AWS SQS queue][aws_sqs].

				[aws_sqs]: https://aws.amazon.com/sqs/
				"""
		}
	}
	tls_options: {
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
