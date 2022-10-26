package metadata

base: components: sources: dnstap: configuration: {
	host_key: {
		description: """
			Overrides the name of the log field used to add the source path to each event.

			The value will be the socket path itself.

			By default, the [global `log_schema.host_key` option][global_host_key] is used.

			[global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
			"""
		required: false
		type: string: syntax: "literal"
	}
	max_frame_handling_tasks: {
		description: "Maximum number of frames that can be processed concurrently."
		required:    false
		type: uint: {}
	}
	max_frame_length: {
		description: "Maximum length, in bytes, that a frame can be."
		required:    false
		type: uint: default: 102400
	}
	multithreaded: {
		description: "Whether or not to concurrently process DNSTAP frames."
		required:    false
		type: bool: {}
	}
	raw_data_only: {
		description: """
			Whether or not to skip parsing/decoding of DNSTAP frames.

			If set to `true`, frames will not be parsed/decoded. The raw frame data will be set as a field on the event
			(called `rawData`) and encoded as a base64 string.
			"""
		required: false
		type: bool: {}
	}
	socket_file_mode: {
		description: """
			Unix file mode bits to be applied to the unix socket file as its designated file permissions.

			Note that the file mode value can be specified in any numeric format supported by your configuration
			language, but it is most intuitive to use an octal number.
			"""
		required: false
		type: uint: {}
	}
	socket_path: {
		description: """
			Absolute path to the socket file to read DNSTAP data from.

			The DNS server must be configured to send its DNSTAP data to this socket file. The socket file will be created,
			if it doesn't already exist, when the source first starts.
			"""
		required: true
		type: string: syntax: "literal"
	}
	socket_receive_buffer_size: {
		description: """
			The size, in bytes, of the receive buffer used for the socket.

			This should not typically needed to be changed.
			"""
		required: false
		type: uint: {}
	}
	socket_send_buffer_size: {
		description: """
			The size, in bytes, of the send buffer used for the socket.

			This should not typically needed to be changed.
			"""
		required: false
		type: uint: {}
	}
}
