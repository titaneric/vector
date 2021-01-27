package metadata

remap: functions: decode_base64: {
	category:    "Codec"
	description: """
		Decodes the `value` (a [Base64](\(urls.base64)) string) into it's original string.
		"""

	arguments: [
		{
			name:        "value"
			description: "The [Base64](\(urls.base64)) data to decode."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid encoded base64 string.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decode Base64 data"
			source: """
				decode_base64("eW91IGhhdmUgc3VjY2Vzc2Z1bGx5IGRlY29kZWQgbWU=")
				"""
			return: "you have successfully decoded me"
		},
	]
}
