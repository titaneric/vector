package metadata

remap: errors: "110": {
	title:       "Invalid argument type"
	description: """
		An argument passed to a [function call expression](\(urls.vrl_expressions)#\(remap.literals.regular_expression.anchor))
		is not a supported type.
		"""
	rationale:   """
		VRL is [type-safe](\(urls.vrl_type_safety)) and requires that types align upon compilation. This contributes
		heavily to VRL's [safety principle](\(urls.vrl_safety)), ensuring that VRL programs are reliable once deployed.
		"""
	resolution: #"""
		You must guarantee the type of the variable by using the appropriate [type](\(urls.vrl_functions)#type) or
		[coercion](\(urls.vrl_functions)#coerce) function.
		"""#

	examples: [...{
		source: #"""
			downcase(.message)
			"""#
		raises: compiletime: #"""
			error: \#(title)
			  ┌─ :1:1
			  │
			1 │ downcase(.message)
			  │          ^^^^^^^^
			  │          │
			  │          this expression resolves to unknown type
			  |          but the parameter "value" expects the exact type "string"
			  │
			"""#
	}]

	examples: [
		{
			"title": "\(title) (guard with defaults)"
			diff: #"""
				+.message = string(.message) ?? ""
				 downcase(.message)
				"""#
		},
		{
			"title": "\(title) (guard with errors)"
			diff: #"""
				 downcase(string!(.message))
				"""#
		},
		{
			"title": "\(title) (guard with if expressions)"
			diff: #"""
				+if is_string(.message) {
				 	downcase(.message)
				+ }
				"""#
		},
	]
}
