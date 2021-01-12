package metadata

remap: functions: starts_with: {
	arguments: [
		{
			name:        "value"
			description: "The string to search."
			required:    true
			type: ["string"]
		},
		{
			name:        "substring"
			description: "The substring `value` must start with."
			required:    true
			type: ["string"]
		},
		{
			name:        "case_sensitive"
			description: "Should the match be case sensitive?"
			required:    false
			type: ["boolean"]
			default: true
		},
	]
	internal_failure_reasons: []
	return: ["boolean"]
	category: "String"
	description: #"""
		Determines if a given `valuye` begins with the given `substring`.
		"""#
	examples: [
		{
			title: "String starts with (case sensitive)"
			input: log: message: #"The Needle In The Haystack"#
			source: #"""
				.starts = starts_with(.message, \"The Needle\")
				"""#
			output: input & {log: starts: true}
		},
		{
			title: "String starts with (case insensitive)"
			input: log: message: #"The Needle In The Haystack"#
			source: #"""
				.starts = starts_with(.message, \"the needle\", case_sensitive: false)
				"""#
			output: input & {log: starts: true}
		},
	]
}
