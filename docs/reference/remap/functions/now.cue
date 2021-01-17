package metadata

remap: functions: now: {
	arguments: []
	internal_failure_reasons: []
	return: ["timestamp"]
	category: "Timestamp"
	description: #"""
		Returns the current timestamp in the UTC timezone with nanosecond precision.
		"""#
	examples: [
		{
			title: "Generate a current timestamp"
			input: log: {}
			source: #"""
				.timestamp = now()
				"""#
			output: log: timestamp: "2020-10-21T20:53:12.212221Z"
		},
	]
}
