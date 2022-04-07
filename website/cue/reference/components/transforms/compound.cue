package metadata

components: transforms: compound: {
	title: "Compound"

	description: """
		Defines an ordered chain of child transforms that will be applied sequentially
		on incoming events.
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {}

	support: {
		requirements: []
		notices: []
		warnings: []
	}

	configuration: {
		steps: {
			description: """
				A list of transforms configurations' representing the chain of transforms to be applied on incoming
				events. All transforms in the chain can then be referenced as an input by other components with the name
				`<transform_name>.<nested_transform_name>`. All transforms in the chain also generate internal metrics
				as if they were configured separately.
				"""
			required: true
			type: array: {
				items: type: object: {
					options: {
						id: {
							common:      true
							description: "The ID to use for the transform. This will show up in metrics and logs in the format: `<compound transform id>.<step id>`. If not set, the index of the step will be used as its id."
							required:    false
							type: string: {
								default: null
								examples: ["my_filter_transform"]
							}
						}
						"*": {
							description: """
							Any valid transform configuration. See [transforms documentation](\(urls.vector_transforms))
							for the list of available transforms and their configuration.
							"""
							required:    true
							type: object: {}
						}
					}
				}
			}
		}
	}

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: true
	}

	examples: [
		{
			title: "Filter by log level and reformat"
			configuration: """
				[transforms.chain]
				type = "compound"

				[[transforms.chain.steps]]
				type = "filter"
				condition = '.level == "debug"'

				[[transforms.chain.steps]]
				type = "remap"
				source = '''
					.message, _ = "[" + del(.level) + "] " +  .message
				'''
				"""
			input: [
				{
					log: {
						level:   "debug"
						message: "I'm a noisy debug log"
					}
				},
				{
					log: {
						level:   "info"
						message: "I'm a normal info log"
					}
				},
			]
			output: [
				{
					log: {
						message: "[debug] I'm a noisy debug log"
					}
				},
			]
		},
	]
}
