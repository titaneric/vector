package metadata

base: components: transforms: lua: configuration: {
	hooks: {
		description: """
			Lifecycle hooks.

			These hooks can be set to perform additional processing during the lifecycle of the transform.
			"""
		required: true
		type: object: options: {
			init: {
				description: """
					A function which is called when the first event comes, before calling `hooks.process`.

					It can produce new events using the `emit` function.

					This can either be inline Lua that defines a closure to use, or the name of the Lua function to call. In both
					cases, the closure/function takes a single parameter, `emit`, which is a reference to a function for emitting events.
					"""
				required: false
				type: string: {
					examples: ["""
						function (emit)
						\t-- Custom Lua code here
						end
						""", "init"]
					syntax: "literal"
				}
			}
			process: {
				description: """
					A function which is called for each incoming event.

					It can produce new events using the `emit` function.

					This can either be inline Lua that defines a closure to use, or the name of the Lua function to call. In both
					cases, the closure/function takes two parameters. The first parameter, `event`, is the event being processed,
					while the second parameter, `emit`, is a reference to a function for emitting events.
					"""
				required: true
				type: string: {
					examples: ["""
						function (event, emit)
						\tevent.log.field = "value" -- set value of a field
						\tevent.log.another_field = nil -- remove field
						\tevent.log.first, event.log.second = nil, event.log.first -- rename field
						\t-- Very important! Emit the processed event.
						\temit(event)
						end
						""", "process"]
					syntax: "literal"
				}
			}
			shutdown: {
				description: """
					A function which is called when Vector is stopped.

					It can produce new events using the `emit` function.

					This can either be inline Lua that defines a closure to use, or the name of the Lua function to call. In both
					cases, the closure/function takes a single parameter, `emit`, which is a reference to a function for emitting events.
					"""
				required: false
				type: string: {
					examples: ["""
						function (emit)
						\t-- Custom Lua code here
						end
						""", "shutdown"]
					syntax: "literal"
				}
			}
		}
	}
	search_dirs: {
		description: """
			A list of directories to search when loading a Lua file via the `require` function.

			If not specified, the modules are looked up in the directories of Vector’s configs.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: {
				examples: ["/etc/vector/lua"]
				syntax: "literal"
			}
		}
	}
	source: {
		description: """
			The Lua program to initialize the transform with.

			The program can be used to to import external dependencies, as well as define the functions
			used for the various lifecycle hooks. However, it's not strictly required, as the lifecycle
			hooks can be configured directly with inline Lua source for each respective hook.
			"""
		required: false
		type: string: {
			examples: ["""
				function init()
				\tcount = 0
				end

				function process()
				\tcount = count + 1
				end

				function timer_handler(emit)
				\temit(make_counter(counter))
				\tcounter = 0
				end

				function shutdown(emit)
				\temit(make_counter(counter))
				end

				function make_counter(value)
				\treturn metric = {
				\t\tname = "event_counter",
				\t\tkind = "incremental",
				\t\ttimestamp = os.date("!*t"),
				\t\tcounter = {
				\t\t\tvalue = value
				\t\t}
				 \t}
				end
				""", """
				-- external file with hooks and timers defined
				require('custom_module')
				"""]
			syntax: "literal"
		}
	}
	timers: {
		description: "A list of timers which should be configured and executed periodically."
		required:    false
		type: array: {
			default: []
			items: type: object: options: {
				handler: {
					description: """
						The handler function which is called when the timer ticks.

						It can produce new events using the `emit` function.

						This can either be inline Lua that defines a closure to use, or the name of the Lua function
						to call. In both cases, the closure/function takes a single parameter, `emit`, which is a
						reference to a function for emitting events.
						"""
					required: true
					type: string: {
						examples: ["timer_handler"]
						syntax: "literal"
					}
				}
				interval_seconds: {
					description: "The interval to execute the handler, in seconds."
					required:    true
					type: uint: unit: "seconds"
				}
			}
		}
	}
	version: {
		description: """
			Transform API version.

			Specifying this version ensures that Vector does not break backward compatibility.
			"""
		required: true
		type: string: enum: {
			"1": """
				Lua transform API version 1.

				This version is deprecated and will be removed in a future version.
				"""
			"2": "Lua transform API version 2."
		}
	}
}
