package metadata

base: components: sources: docker_logs: configuration: {
	auto_partial_merge: {
		description: "Enables automatic merging of partial events."
		required:    false
		type: bool: default: true
	}
	docker_host: {
		description: """
			Docker host to connect to.

			Use an HTTPS URL to enable TLS encryption.

			If absent, Vector will try to use `DOCKER_HOST` environment variable. If `DOCKER_HOST` is also absent, Vector will use default Docker local socket (`/var/run/docker.sock` on Unix platforms, `//./pipe/docker_engine` on Windows).
			"""
		required: false
		type: string: syntax: "literal"
	}
	exclude_containers: {
		description: """
			A list of container IDs or names of containers to exclude from log collection.

			Matching is prefix first, so specifying a value of `foo` would match any container named `foo` as well as any
			container whose name started with `foo`. This applies equally whether matching container IDs or names.

			By default, the source will collect logs for all containers. If `exclude_containers` is configured, any
			container that matches a configured exclusion will be excluded even if it is also included via
			`include_containers`, so care should be taken when utilizing prefix matches as they cannot be overridden by a
			corresponding entry in `include_containers` e.g. excluding `foo` by attempting to include `foo-specific-id`.

			This can be used in conjunction with `include_containers`.
			"""
		required: false
		type: array: items: type: string: syntax: "literal"
	}
	host_key: {
		description: """
			Overrides the name of the log field used to add the current hostname to each event.

			The value will be the current hostname for wherever Vector is running.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: {
			default: "host"
			syntax:  "literal"
		}
	}
	include_containers: {
		description: """
			A list of container IDs or names of containers to include in log collection.

			Matching is prefix first, so specifying a value of `foo` would match any container named `foo` as well as any
			container whose name started with `foo`. This applies equally whether matching container IDs or names.

			By default, the source will collect logs for all containers. If `include_containers` is configured, only
			containers that match a configured inclusion and are also not excluded will be matched.

			This can be used in conjunction with `include_containers`.
			"""
		required: false
		type: array: items: type: string: syntax: "literal"
	}
	include_images: {
		description: """
			A list of image names to match against.

			If not provided, all images will be included.
			"""
		required: false
		type: array: items: type: string: syntax: "literal"
	}
	include_labels: {
		description: """
			A list of container object labels to match against when filtering running containers.

			Labels should follow the syntax described in the [Docker object labels](https://docs.docker.com/config/labels-custom-metadata/) documentation.
			"""
		required: false
		type: array: items: type: string: syntax: "literal"
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
	partial_event_marker_field: {
		description: """
			Overrides the name of the log field used to mark an event as partial.

			If `auto_partial_merge` is disabled, partial events will be emitted with a log field, controlled by this
			configuration value, is set, indicating that the event is not complete.

			By default, `"_partial"` is used.
			"""
		required: false
		type: string: {
			default: "_partial"
			syntax:  "literal"
		}
	}
	retry_backoff_secs: {
		description: "The amount of time, in seconds, to wait before retrying after an error."
		required:    false
		type: uint: default: 2
	}
	tls: {
		description: """
			Configuration of TLS when connecting to the Docker daemon.

			Only relevant when connecting to Docker via an HTTPS URL.

			If not configured, Vector will try to use environment variable `DOCKER_CERT_PATH` and then` DOCKER_CONFIG`. If both environment variables are absent, Vector will try to read certificates in `~/.docker/`.
			"""
		required: false
		type: object: options: {
			ca_file: {
				description: "Path to the CA certificate file."
				required:    true
				type: string: syntax: "literal"
			}
			crt_file: {
				description: "Path to the TLS certificate file."
				required:    true
				type: string: syntax: "literal"
			}
			key_file: {
				description: "Path to the TLS key file."
				required:    true
				type: string: syntax: "literal"
			}
		}
	}
}
