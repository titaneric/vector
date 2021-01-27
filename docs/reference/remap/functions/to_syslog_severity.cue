package metadata

remap: functions: to_syslog_severity: {
	category:    "Coerce"
	description: """
		Coerces the `value`, a Syslog [log level keyword](\(urls.syslog_levels)), into an Syslog integer
		severity level (`0` to `7`).
		"""

	arguments: [
		{
			name:        "value"
			description: "The Syslog level keyword to convert."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid Syslog level keyword",
	]
	return: {
		types: ["integer"]
		rules: [
			"The now-deprecated keywords `panic`, `error`, and `warn` are converted to `0`, `3`, and `4` respectively.",
		]
	}

	examples: [
		{
			title: "Coerce to Syslog serverity"
			source: """
				to_syslog_severity("alert")
				"""
			return: 1
		},
	]
}
