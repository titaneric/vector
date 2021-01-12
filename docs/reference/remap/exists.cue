package metadata

remap: functions: exists: {
	arguments: [
		{
			name:        "path"
			description: "The paths of the fields to check."
			required:    true
			multiple:    false
			type: ["path"]
		},
	]
	internal_failure_reason: null
	return: ["boolean"]
	category: "Event"
	description: #"""
		Checks if the given `path` exists. Nested paths and arrays can also be checked.
		"""#
	examples: [
		{
			title: "Field exists"
			input: log: field: 1
			source: #"""
				.exists = exists(.field)
				.doesntexist = exists(.field2)
				"""#
			output: input & {log: {
				exists:      true
				doesntexist: false
			}}
		},
		{
			title: "Array element exists"
			input: log: array: [1, 2, 3]
			source: #"""
				.exists = exists(.array[2])
				.doesntexist = exists(.array[3])
				"""#
			output: input & {log: {
				exists:      true
				doesntexist: false
			}}
		},
	]
}
