package metadata

components: sources: host_metrics: {
	title:       "Host Metrics"
	description: "The host metrics source examines system data sources on the local system and generates metrics describing utilization of various system resources."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon"]
		development:   "beta"
		egress_method: "batch"
	}

	features: {
		collect: {
			checkpoint: enabled: false
			from: service: {
				name:     "host"
				thing:    "a \(name)"
				url:      urls.host
				versions: null
			}
		}
		multiline: enabled: false
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		notices: []
		requirements: []
		warnings: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		collectors: {
			description: "The list of host metric collector services to use. Defaults to all collectors."
			common:      true
			required:    false
			type: array: {
				default: ["cpu", "disk", "filesystem", "load", "memory", "network"]
				items: type: string: enum: {
					cpu:        "Metrics related to CPU utilization."
					disk:       "Metrics related to disk I/O utilization."
					filesystem: "Metrics related to filesystem space utilization."
					load:       "Load average metrics (UNIX only)."
					memory:     "Metrics related to memory utilization."
					network:    "Metrics related to network utilization."
				}
			}
		}
		namespace: {
			description: "The namespace of metrics. Disabled if empty."
			common:      false
			required:    false
			type: string: default: "host"
		}
		scrape_interval_secs: {
			description: "The interval between metric gathering, in seconds."
			common:      true
			required:    false
			type: uint: {
				default: 15
				unit:    "seconds"
			}
		}
		disk: {
			common:      false
			description: #"Options for the "disk" metrics collector."#
			required:    false
			type: object: options: {
				devices: {
					common:      false
					required:    false
					description: "Lists of device name patterns to include or exclude."
					type: object: options: {
						includes: {
							required: false
							common:   false
							description: """
								The list of device name patterns for which to gather I/O utilization metrics.
								Defaults to including all devices.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: ["*"]
								items: type: string: examples: ["sda", "dm-*"]
							}
						}
						excludes: {
							required: false
							common:   false
							description: """
								The list of device name patterns for which to gather I/O utilization metrics.
								Defaults to excluding no devices.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: []
								items: type: string: examples: ["sda", "dm-*"]
							}
						}
					}
				}
			}
		}
		filesystem: {
			common:      false
			description: #"Options for the "filesystem" metrics collector."#
			required:    false
			type: object: options: {
				devices: {
					common:      false
					required:    false
					description: "Lists of device name patterns to include or exclude."
					type: object: options: {
						includes: {
							required: false
							common:   false
							description: """
								The list of device name patterns for which to gather usage metrics.
								Defaults to including all devices.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: ["*"]
								items: type: string: examples: ["sda", "dm-*"]
							}
						}
						excludes: {
							required: false
							common:   false
							description: """
								The list of device name patterns for which to gather usage metrics.
								Defaults to excluding no devices.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: []
								items: type: string: examples: ["sda", "dm-*"]
							}
						}
					}
				}
				filesystems: {
					common:      false
					required:    false
					description: "Lists of filesystem name patterns to include or exclude."
					type: object: options: {
						includes: {
							required: false
							common:   false
							description: """
								The list of filesystem name patterns for which to gather usage metrics.
								Defaults to including all filesystems.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: ["*"]
								items: type: string: examples: ["ntfs", "ext*"]
							}
						}
						excludes: {
							required: false
							common:   false
							description: """
								The list of filesystem name patterns for which to gather usage metrics.
								Defaults to excluding no filesystems.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: []
								items: type: string: examples: ["ntfs", "ext*"]
							}
						}
					}
				}
				mountpoints: {
					common:      false
					required:    false
					description: "Lists of mount point path patterns to include or exclude."
					type: object: options: {
						includes: {
							required: false
							common:   false
							description: """
								The list of mount point path patterns for which to gather usage metrics.
								Defaults to including all mount points.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: ["*"]
								items: type: string: examples: ["/home", "/raid*"]
							}
						}
						excludes: {
							required: false
							common:   false
							description: """
								The list of mount point path patterns for which to gather usage metrics.
								Defaults to excluding no mount points.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: []
								items: type: string: examples: ["/home", "/raid*"]
							}
						}
					}
				}
			}
		}
		network: {
			common:      false
			description: #"Options for the "network" metrics collector."#
			required:    false
			type: object: options: {
				devices: {
					common:      false
					required:    false
					description: "Lists of device name patterns to include or exclude."
					type: object: options: {
						includes: {
							required: false
							common:   false
							description: """
								The list of device name patterns for which to gather network utilization metrics.
								Defaults to including all devices.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: ["*"]
								items: type: string: examples: ["sda", "dm-*"]
							}
						}
						excludes: {
							required: false
							common:   false
							description: """
								The list of device name patterns for which to gather network utilization metrics.
								Defaults to excluding no devices.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: []
								items: type: string: examples: ["sda", "dm-*"]
							}
						}
					}
				}
			}
		}
	}

	output: metrics: {
		_host_metrics_tags: {
			collector: {
				description: "Which collector this metric comes from."
				required:    true
			}
			host: {
				description: "The hostname of the originating system."
				required:    true
				examples: [_values.local_host]
			}
		}

		// Host CPU
		host_cpu_seconds_total: _host & {
			description: "The number of CPU seconds accumulated in different operating modes."
			type:        "counter"
			tags:        _host_metrics_tags & {
				collector: examples: ["cpu"]
				cpu: {
					description: "The index of the CPU core or socket."
					required:    true
					examples: ["1"]
				}
				mode: {
					description: "Which mode the CPU was running in during the given time."
					required:    true
					examples: ["idle", "system", "user", "nice"]
				}
			}
		}

		// Host disk
		host_disk_read_bytes_total:       _host & _disk_counter & {description: "The accumulated number of bytes read in."}
		host_disk_reads_completed_total:  _host & _disk_counter & {description: "The accumulated number of read operations completed."}
		host_disk_written_bytes_total:    _host & _disk_counter & {description: "The accumulated number of bytes written out."}
		host_disk_writes_completed_total: _host & _disk_counter & {description: "The accumulated number of write operations completed."}

		// Host filesystem
		host_filesystem_free_bytes:  _host & _filesystem_bytes & {description: "The number of bytes free on the named filesystem."}
		host_filesystem_total_bytes: _host & _filesystem_bytes & {description: "The total number of bytes in the named filesystem."}
		host_filesystem_used_bytes:  _host & _filesystem_bytes & {description: "The number of bytes used on the named filesystem."}

		// Host load
		host_load1:  _host & _loadavg & {description: "System load averaged over the last 1 second."}
		host_load5:  _host & _loadavg & {description: "System load averaged over the last 5 seconds."}
		host_load15: _host & _loadavg & {description: "System load averaged over the last 15 seconds."}

		// Host memory
		host_memory_active_bytes:           _host & _memory_gauge & _memory_nowin & {description: "The number of bytes of active main memory."}
		host_memory_available_bytes:        _host & _memory_gauge & {description:                 "The number of bytes of main memory available."}
		host_memory_buffers_bytes:          _host & _memory_linux & {description:                 "The number of bytes of main memory used by buffers."}
		host_memory_cached_bytes:           _host & _memory_linux & {description:                 "The number of bytes of main memory used by cached blocks."}
		host_memory_free_bytes:             _host & _memory_gauge & {description:                 "The number of bytes of main memory not used."}
		host_memory_inactive_bytes:         _host & _memory_macos & {description:                 "The number of bytes of main memory that is not active."}
		host_memory_shared_bytes:           _host & _memory_linux & {description:                 "The number of bytes of main memory shared between processes."}
		host_memory_swap_free_bytes:        _host & _memory_gauge & {description:                 "The number of free bytes of swap space."}
		host_memory_swapped_in_bytes_total: _host & _memory_counter & _memory_nowin & {
			description: "The number of bytes that have been swapped in to main memory."
		}
		host_memory_swapped_out_bytes_total: _host & _memory_counter & _memory_nowin & {
			description: "The number of bytes that have been swapped out from main memory."
		}
		host_memory_swap_total_bytes: _host & _memory_gauge & {description: "The total number of bytes of swap space."}
		host_memory_swap_used_bytes:  _host & _memory_gauge & {description: "The number of used bytes of swap space."}
		host_memory_total_bytes:      _host & _memory_gauge & {description: "The total number of bytes of main memory."}
		host_memory_used_bytes:       _host & _memory_linux & {description: "The number of bytes of main memory used by programs or caches."}
		host_memory_wired_bytes:      _host & _memory_macos & {description: "The number of wired bytes of main memory."}

		// Host network
		host_network_receive_bytes_total:         _host & _network_gauge & {description: "The number of bytes received on this interface."}
		host_network_receive_errs_total:          _host & _network_gauge & {description: "The number of errors encountered during receives on this interface."}
		host_network_receive_packets_total:       _host & _network_gauge & {description: "The number of packets received on this interface."}
		host_network_transmit_bytes_total:        _host & _network_gauge & {description: "The number of bytes transmitted on this interface."}
		host_network_transmit_errs_total:         _host & _network_gauge & {description: "The number of errors encountered during transmits on this interface."}
		host_network_transmit_packets_drop_total: _host & _network_nomac & {description: "The number of packets dropped during transmits on this interface."}
		host_network_transmit_packets_total:      _host & _network_nomac & {description: "The number of packets transmitted on this interface."}

		// Helpers
		_host: {
			default_namespace: "host"
		}

		_disk_device: {
			description: "The disk device name."
			required:    true
			examples: ["sda", "sda1", "dm-1"]
		}
		_disk_counter: {
			type: "counter"
			tags: _host_metrics_tags & {
				collector: examples: ["disk"]
				device: _disk_device
			}
		}
		_filesystem_bytes: {
			type: "gauge"
			tags: _host_metrics_tags & {
				collector: examples: ["filesystem"]
				device: _disk_device
				filesystem: {
					description: "The name of the filesystem type."
					required:    true
					examples: ["ext4", "ntfs"]
				}
			}
		}
		_loadavg: {
			type: "gauge"
			tags: _host_metrics_tags & {
				collector: examples: ["loadavg"]
			}
			relevant_when: "OS is not Windows"
		}
		_memory_counter: {
			type: "counter"
			tags: _host_metrics_tags & {
				collector: examples: ["memory"]
			}
		}
		_memory_gauge: {
			type: "gauge"
			tags: _host_metrics_tags & {
				collector: examples: ["memory"]
			}
		}
		_memory_linux: _memory_gauge & {relevant_when: "OS is Linux"}
		_memory_macos: _memory_gauge & {relevant_when: "OS is macOS X"}
		_memory_nowin: {relevant_when: "OS is not Windows"}
		_network_gauge: {
			type: "gauge"
			tags: _host_metrics_tags & {
				collector: examples: ["network"]
				device: {
					description: "The network interface device name."
					required:    true
					examples: ["eth0", "enp5s3"]
				}
			}
		}
		_network_nomac: _network_gauge & {relevant_when: "OS is not macOS"}
	}
}
