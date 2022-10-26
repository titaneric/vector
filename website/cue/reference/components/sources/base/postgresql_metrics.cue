package metadata

base: components: sources: postgresql_metrics: configuration: {
	endpoints: {
		description: """
			A list of PostgreSQL instances to scrape.

			Each endpoint must be in the [Connection URI
			format](https://www.postgresql.org/docs/current/libpq-connect.html#id-1.7.3.8.3.6).
			"""
		required: false
		type: array: {
			default: []
			items: type: string: syntax: "literal"
		}
	}
	exclude_databases: {
		description: """
			A list of databases to match (by using [POSIX Regular
			Expressions](https://www.postgresql.org/docs/current/functions-matching.html#FUNCTIONS-POSIX-REGEXP)) against
			the `datname` column for which you don’t want to collect metrics from.

			Specifying `""` will include metrics where `datname` is `NULL`.

			This can be used in conjunction with `include_databases`.
			"""
		required: false
		type: array: items: type: string: syntax: "literal"
	}
	include_databases: {
		description: """
			A list of databases to match (by using [POSIX Regular
			Expressions](https://www.postgresql.org/docs/current/functions-matching.html#FUNCTIONS-POSIX-REGEXP)) against
			the `datname` column for which you want to collect metrics from.

			If not set, metrics are collected from all databases. Specifying `""` will include metrics where `datname` is
			`NULL`.

			This can be used in conjunction with `exclude_databases`.
			"""
		required: false
		type: array: items: type: string: syntax: "literal"
	}
	namespace: {
		description: """
			Overrides the default namespace for the metrics emitted by the source.

			By default, `postgresql` is used.
			"""
		required: false
		type: string: {
			default: "postgresql"
			syntax:  "literal"
		}
	}
	scrape_interval_secs: {
		description: "The interval between scrapes, in seconds."
		required:    false
		type: uint: default: 15
	}
	tls: {
		description: "Configuration of TLS when connecting to PostgreSQL."
		required:    false
		type: object: options: ca_file: {
			description: """
				Absolute path to an additional CA certificate file.

				The certficate must be in the DER or PEM (X.509) format.
				"""
			required: true
			type: string: syntax: "literal"
		}
	}
}
