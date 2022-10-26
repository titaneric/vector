package metadata

base: components: transforms: reduce: configuration: {
	ends_when: {
		description: """
			A condition used to distinguish the final event of a transaction.

			If this condition resolves to `true` for an event, the current transaction is immediately
			flushed with this event.
			"""
		required: false
		type: condition: {}
	}
	expire_after_ms: {
		description: """
			The maximum period of time to wait after the last event is received, in milliseconds, before
			a combined event should be considered complete.
			"""
		required: false
		type: uint: {
			default: 30000
			unit:    "milliseconds"
		}
	}
	flush_period_ms: {
		description: "The interval to check for and flush any expired events, in milliseconds."
		required:    false
		type: uint: {
			default: 1000
			unit:    "milliseconds"
		}
	}
	group_by: {
		description: """
			An ordered list of fields by which to group events.

			Each group with matching values for the specified keys is reduced independently, allowing
			you to keep independent event streams separate. When no fields are specified, all events
			will be combined in a single group.

			For example, if `group_by = ["host", "region"]`, then all incoming events that have the same
			host and region will be grouped together before being reduced.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: {
				examples: ["request_id", "user_id", "transaction_id"]
				syntax: "literal"
			}
		}
	}
	merge_strategies: {
		description: """
			A map of field names to custom merge strategies.

			For each field specified, the given strategy will be used for combining events rather than
			the default behavior.

			The default behavior is as follows:

			- The first value of a string field is kept, subsequent values are discarded.
			- For timestamp fields the first is kept and a new field `[field-name]_end` is added with
			  the last received timestamp value.
			- Numeric values are summed.
			"""
		required: false
		type: object: options: "*": {
			description: """
				A map of field names to custom merge strategies.

				For each field specified, the given strategy will be used for combining events rather than
				the default behavior.

				The default behavior is as follows:

				- The first value of a string field is kept, subsequent values are discarded.
				- For timestamp fields the first is kept and a new field `[field-name]_end` is added with
				  the last received timestamp value.
				- Numeric values are summed.
				"""
			required: true
			type: string: enum: {
				array:          "Append each value to an array."
				concat:         "Concatenate each string value, delimited with a space."
				concat_newline: "Concatenate each string value, delimited with a newline."
				concat_raw:     "Concatenate each string, without a delimiter."
				discard:        "Discard all but the first value found."
				flat_unique:    "Create a flattened array of all unique values."
				longest_array:  "Keep the longest array seen."
				max:            "Keep the maximum numeric value seen."
				min:            "Keep the minimum numeric value seen."
				retain: """
					Discard all but the last value found.

					Works as a way to coalesce by not retaining `null`.
					"""
				shortest_array: "Keep the shortest array seen."
				sum:            "Sum all numeric values."
			}
		}
	}
	starts_when: {
		description: """
			A condition used to distinguish the first event of a transaction.

			If this condition resolves to `true` for an event, the previous transaction is flushed
			(without this event) and a new transaction is started.
			"""
		required: false
		type: condition: {}
	}
}
