package metadata

#Remap: {
	#Characteristic: {
		title:       string
		description: string
	}

	#Example: {
		title:   string
		input?:  #Event
		source:  string
		raises?: string

		if raises == _|_ {
			return?: _
			output?: #Event
		}
	}

	#Type: "any" | "array" | "boolean" | "float" | "integer" | "map" | "null" | "path" | "string" | "regex" | "timestamp"

	concepts:    _
	description: string
	expressions: _
	features:    _
	functions:   _
	literals:    _
	principles:  _
	real_world_examples: [#Example, ...#Example]
}

remap: #Remap & {
	description: #"""
		**Vector Remap Language** (VRL) is an [expression-oriented](\#(urls.expression_oriented_language)) language
		designed for expressing obervability data (logs and metrics) transformations. It features a simple
		[syntax](\#(urls.vrl_spec)) and a rich set of built-in [functions](\#(urls.vrl_functions)) tailored
		specifically to observability use cases.

		For a more in-depth picture, see the [announcement blog post](\#(urls.vrl_announcement)) for more details.
		"""#

	real_world_examples: [
		{
			title: "Parse Syslog logs"
			input: log: "<102>1 2020-12-22T15:22:31.111Z vector-user.biz su 2666 ID389 - Something went wrong"
			source: """
				. = parse_syslog!(.message)
				"""
			output: log: {
				appname:   "su"
				facility:  "ntp"
				hostname:  "vector-user.biz"
				message:   "Something went wrong"
				msgid:     "ID389"
				procid:    2666
				severity:  "info"
				timestamp: "2020-12-22 15:22:31.111 UTC"
			}
		},
		{
			title: "Parse key/value logs"
			input: log: message: "@timestamp=\"Sun Jan 10 16:47:39 EST 2021\" level=info msg=\"Stopping all fetchers\" tag#production=stopping_fetchers id=ConsumerFetcherManager-1382721708341 module=kafka.consumer.ConsumerFetcherManager"
			source: """
				. = parse_key_value!(.message)
				"""
			output: log: {
				"@timestamp":     "Sun Jan 10 16:47:39 EST 2021"
				level:            "info"
				msg:              "Stopping all fetchers"
				"tag#production": "stopping_fetchers"
				id:               "ConsumerFetcherManager-1382721708341"
				module:           "kafka.consumer.ConsumerFetcherManager"
			}
		},
		{
			title: "Parse custom logs"
			input: log: message: #"2021/01/20 06:39:15 [error] 17755#17755: *3569904 open() "/usr/share/nginx/html/test.php" failed (2: No such file or directory), client: xxx.xxx.xxx.xxx, server: localhost, request: "GET /test.php HTTP/1.1", host: "yyy.yyy.yyy.yyy""#
			source: #"""
				. = parse_regex!(.message, /^(?P<timestamp>\d+/\d+/\d+ \d+:\d+:\d+) \[(?P<severity>\w+)\] (?P<pid>\d+)#(?P<tid>\d+):(?: \*(?P<connid>\d+))? (?P<message>.*)$/)

				# Coerce parsed fields
				.timestamp = parse_timestamp(.timestamp, "%Y/%m/%d %H:%M:%S") ?? now()
				.pid = to_int(.pid)
				.tid = to_int(.tid)

				# Extract structured data
				message_parts = split(.message, ", ", limit: 2)
				structured = parse_key_value(message_parts[1], key_value_delimiter: ":", field_delimiter: ",") ?? {}
				.message = message_parts[0]
				. = merge(., structured)
				"""#
			output: log: {
				timestamp: "2021/01/20 06:39:15"
				severity:  "error"
				pid:       "17755"
				tid:       "17755"
				connid:    "3569904"
				message:   #"open() "/usr/share/nginx/html/test.php" failed (2: No such file or directory)"#
				client:    "xxx.xxx.xxx.xxx"
				server:    "localhost"
				request:   "GET /test.php HTTP/1.1"
				host:      "yyy.yyy.yyy.yyy"
			}
		},
	]
}
