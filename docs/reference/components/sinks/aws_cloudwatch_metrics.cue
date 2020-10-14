package metadata

components: sinks: aws_cloudwatch_metrics: {
	title:             "AWS Cloudwatch Metrics"
	short_description: "Streams metric events to [Amazon Web Service's CloudWatch Metrics service][urls.aws_cloudwatch_metrics] via the [`PutMetricData` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_PutMetricData.html)."
	long_description:  "[Amazon CloudWatch][urls.aws_cloudwatch] is a monitoring and management service that provides data and actionable insights for AWS, hybrid, and on-premises applications and infrastructure resources. With CloudWatch, you can collect and access all your performance and operational data in the form of logs and metrics from a single platform."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		function:      "transmit"
		service_providers: ["AWS"]
	}

	features: {
		batch: {
			enabled:      true
			common:       false
			max_bytes:    null
			max_events:   20
			timeout_secs: 1
		}
		buffer: enabled: false
		compression: {
			enabled: true
			default: null
			gzip:    true
		}
		encoding: codec: enabled: false
		healthcheck: enabled: true
		request: enabled:     false
		tls: enabled:         false
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
		warnings: [
			#"""
				Gauge values are persisted between flushes. On Vector start up each
				gauge is assumed to have zero, 0.0, value, that can be updated
				explicitly by the consequent absolute, not delta, gauge observation,
				or by delta increments/decrements. Delta gauges are considered an
				advanced feature useful in a distributed setting, however they
				should be used with care.
				"""#,
		]
		notices: [
			#"""
				CloudWatch Metrics types are organized not by their semantics, but
				by storage properties:

				* Statistic Sets
				* Data Points

				In Vector only the latter is used to allow lossless statistics
				calculations on CloudWatch side.
				"""#,
		]
	}

	configuration: {
		namespace: {
			description: "A [namespace](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/cloudwatch_concepts.html#Namespace) that will isolate different metrics from each other."
			required:    true
			warnings: []
			type: string: {
				examples: ["service"]
			}
		}
	}

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          false
			summary:      true
		}
	}
}
