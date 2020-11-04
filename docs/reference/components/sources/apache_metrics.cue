package metadata

components: sources: apache_metrics: {
	_config_path: "/etc/apache2/httpd.conf"
	_path:        "/server-status"

	title: "Apache HTTP Server (HTTPD) Metrics"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar"]
		development:   "beta"
		egress_method: "batch"
	}

	features: {
		multiline: enabled: false
		collect: {
			checkpoint: enabled: false
			from: {
				name:     "Apache HTTP server (HTTPD)"
				thing:    "an \(name)"
				url:      urls.apache
				versions: null

				interface: {
					socket: {
						api: {
							title: "Apache HTTP Server Status Module"
							url:   urls.apache_mod_status
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "disabled"
					}
				}

				setup: [
					"""
						[Install the Apache HTTP server](\(urls.apache_install)).
						""",
					"""
						Enable the [Apache Status module](\(urls.apache_mod_status))
						in your Apache config:

						```text file="\(_config_path)"
						<Location "\(_path)">
						    SetHandler server-status
						    Require host example.com
						</Location>
						```
						""",
					"""
						Optionally enable [`ExtendedStatus` option](\(urls.apache_extended_status))
						for more detailed metrics (see [Output](#output)). Note,
						this defaults to `On` in Apache >= 2.3.6.

						```text file="\(_config_path)"
						ExtendedStatus On
						```
						""",
					"""
						Start or reload Apache to apply the config changes.
						""",
				]
			}
		}
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		endpoints: {
			description: "mod_status endpoints to scrape metrics from."
			required:    true
			type: array: {
				items: type: string: examples: ["http://localhost:8080/server-status/?auto"]
			}
		}
		interval_secs: {
			description: "The interval between scrapes."
			common:      true
			required:    false
			type: uint: {
				default: 15
				unit:    "seconds"
			}
		}
		namespace: {
			description: "The namespace of the metric. Disabled if empty."
			required:    false
			common:      false
			warnings: []
			type: string: {
				default: "apache"
			}
		}
	}

	output: metrics: {
		_endpoint: {
			description: "The absolute path of originating file."
			required:    true
			examples: ["http://localhost:8080/server-status?auto"]
		}
		_host: {
			description: "The hostname of the Apache HTTP server"
			required:    true
			examples: [_values.local_host]
		}
		access_total: {
			description:   "The total number of time the Apache server has been accessed."
			relevant_when: "`ExtendedStatus On`"
			type:          "counter"
			tags: {
				endpoint: _endpoint
				host:     _host
			}
		}
		connections: {
			description: "The total number of time the Apache server has been accessed."
			type:        "gauge"
			tags: {
				endpoint: _endpoint
				host:     _host
				state: {
					description: "The state of the connection"
					required:    true
					examples: ["closing", "keepalive", "total", "writing"]
				}
			}
		}
		cpu_load: {
			description:   "The current CPU of the Apache server."
			relevant_when: "`ExtendedStatus On`"
			type:          "gauge"
			tags: {
				endpoint: _endpoint
				host:     _host
			}
		}
		cpu_seconds_total: {
			description:   "The CPU time of various Apache processes."
			relevant_when: "`ExtendedStatus On`"
			type:          "counter"
			tags: {
				endpoint: _endpoint
				host:     _host
				type: {
					description: "The state of the connection"
					required:    true
					examples: ["children_system", "children_user", "system", "user"]
				}
			}
		}
		duration_seconds_total: {
			description:   "The amount of time the Apache server has been running."
			relevant_when: "`ExtendedStatus On`"
			type:          "counter"
			tags: {
				endpoint: _endpoint
				host:     _host
			}
		}
		scoreboard: {
			description: "The amount of times various Apache server tasks have been run."
			type:        "gauge"
			tags: {
				endpoint: _endpoint
				host:     _host
				state: {
					description: "The connect state"
					required:    true
					examples: ["closing", "dnslookup", "finishing", "idle_cleanup", "keepalive", "logging", "open", "reading", "sending", "starting", "waiting"]
				}
			}
		}
		sent_bytes_total: {
			description:   "The amount of bytes sent by the Apache server."
			relevant_when: "`ExtendedStatus On`"
			type:          "counter"
			tags: {
				endpoint: _endpoint
				host:     _host
			}
		}
		uptime_seconds_total: {
			description: "The amount of time the Apache server has been running."
			type:        "counter"
			tags: {
				endpoint: _endpoint
				host:     _host
			}
		}
		workers: {
			description: "Apache worker statuses."
			type:        "gauge"
			tags: {
				endpoint: _endpoint
				host:     _host
				state: {
					description: "The state of the worker"
					required:    true
					examples: ["busy", "idle"]
				}
			}
		}
		up: {
			description: "If the Apache server is up or not."
			type:        "gauge"
			tags: {
				endpoint: _endpoint
				host:     _host
			}
		}
	}

	how_it_works: {}
}
