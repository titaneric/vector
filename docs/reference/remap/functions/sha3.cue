package metadata

remap: functions: sha3: {
	arguments: [
		{
			name:        "value"
			description: "The string to calculate the hash for."
			required:    true
			type: ["string"]
		},
		{
			name: "variant"
			description: #"""
				The variant of the algorithm to use.
				The allowed variants are:
				- SHA3-224
				- SHA3-256
				- SHA3-384
				- SHA3-512
				"""#
			required: false
			default:  "SHA3-512"
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: ["string"]
	category: "Hash"
	description: #"""
		Calculates a sha3 hash of the provided `value`.
		"""#
	examples: [
		{
			title: "Calaculate sha3 hash"
			input: log: text: #"foo"#
			source: #"""
				.hash = sha3(.text, variant: "SHA3-224")
				"""#
			output: input & {log: hash: "f4f6779e153c391bbd29c95e72b0708e39d9166c7cea51d1f10ef58a"}
		},
	]
}
