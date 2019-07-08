<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/splunk_hec.md.erb
-->

---
description: Batches `log` events to a Splunk HTTP Event Collector.
---

# splunk_hec sink

![][images.splunk_hec_sink]


The `splunk_hec` sink batches [`log`][docs.log_event] events to a [Splunk HTTP Event Collector][url.splunk_hec].

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```toml
[sinks.my_splunk_hec_sink_id]
  # REQUIRED - General
  type = "splunk_hec" # must be: "splunk_hec"
  inputs = ["my-source-id"]

  # OPTIONAL - General
  host = "my-splunk-host.com" # no default
  token = "A94A8FE5CCB19BA61C4C08" # no default

  # OPTIONAL - Batching
  batch_size = 1049000 # default, bytes
  batch_timeout = 1 # default, bytes

  # OPTIONAL - Requests
  encoding = "ndjson" # no default, enum: "ndjson", "text"
  rate_limit_duration = 1 # default, seconds
  rate_limit_num = 10 # default
  request_in_flight_limit = 10 # default
  request_timeout_secs = 60 # default, seconds
  retry_attempts = 5 # default
  retry_backoff_secs = 5 # default, seconds

  # OPTIONAL - Buffer
  [sinks.my_splunk_hec_sink_id.buffer]
    type = "memory" # default, enum: "memory", "disk"
    when_full = "block" # default, enum: "block", "drop_newest"
    max_size = 104900000 # no default
    num_items = 500 # default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```toml
[sinks.<sink-id>]
  # REQUIRED - General
  type = "splunk_hec"
  inputs = ["<string>", ...]

  # OPTIONAL - General
  host = "<string>"
  token = "<string>"

  # OPTIONAL - Batching
  batch_size = <int>
  batch_timeout = <int>

  # OPTIONAL - Requests
  encoding = {"ndjson" | "text"}
  rate_limit_duration = <int>
  rate_limit_num = <int>
  request_in_flight_limit = <int>
  request_timeout_secs = <int>
  retry_attempts = <int>
  retry_backoff_secs = <int>

  # OPTIONAL - Buffer
  [sinks.<sink-id>.buffer]
    type = {"memory" | "disk"}
    when_full = {"block" | "drop_newest"}
    max_size = <int>
    num_items = <int>
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```toml
[sinks.splunk_hec]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "splunk_hec"
  type = "splunk_hec"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # Your Splunk HEC host.
  # 
  # * optional
  # * no default
  host = "my-splunk-host.com"

  # Your Splunk HEC token.
  # 
  # * optional
  # * no default
  token = "A94A8FE5CCB19BA61C4C08"

  #
  # Batching
  #

  # The maximum size of a batch before it is flushed.
  # 
  # * optional
  # * default: 1049000
  # * unit: bytes
  batch_size = 1049000

  # The maximum age of a batch before it is flushed.
  # 
  # * optional
  # * default: 1
  # * unit: bytes
  batch_timeout = 1

  #
  # Requests
  #

  # The encoding format used to serialize the events before flushing.
  # 
  # * optional
  # * no default
  # * enum: "ndjson", "text"
  encoding = "ndjson"
  encoding = "text"

  # The window used for the `request_rate_limit_num` option
  # 
  # * optional
  # * default: 1
  # * unit: seconds
  rate_limit_duration = 1

  # The maximum number of requests allowed within the `rate_limit_duration`
  # window.
  # 
  # * optional
  # * default: 10
  rate_limit_num = 10

  # The maximum number of in-flight requests allowed at any given time.
  # 
  # * optional
  # * default: 10
  request_in_flight_limit = 10

  # The maximum time a request can take before being aborted.
  # 
  # * optional
  # * default: 60
  # * unit: seconds
  request_timeout_secs = 60

  # The maximum number of retries to make for failed requests.
  # 
  # * optional
  # * default: 5
  retry_attempts = 5

  # The amount of time to wait before attempting a failed request again.
  # 
  # * optional
  # * default: 5
  # * unit: seconds
  retry_backoff_secs = 5

  #
  # Buffer
  #

  [sinks.splunk_hec.buffer]
    # The buffer's type / location. `disk` buffers are persistent and will be
    # retained between restarts.
    # 
    # * optional
    # * default: "memory"
    # * enum: "memory", "disk"
    type = "memory"
    type = "disk"

    # The behavior when the buffer becomes full.
    # 
    # * optional
    # * default: "block"
    # * enum: "block", "drop_newest"
    when_full = "block"
    when_full = "drop_newest"

    # Only relevant when `type` is `disk`. The maximum size of the buffer on the
    # disk.
    # 
    # * optional
    # * no default
    max_size = 104900000

    # Only relevant when `type` is `memory`. The maximum number of events allowed
    # in the buffer.
    # 
    # * optional
    # * default: 500
    num_items = 500
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `enum: "splunk_hec"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| **OPTIONAL** - General | | |
| `host` | `string` | Your Splunk HEC host.<br />`no default` `example: "my-splunk-host.com"` |
| `token` | `string` | Your Splunk HEC token.<br />`no default` `example: "A94A8FE5CCB19BA61C4C08"` |
| **OPTIONAL** - Batching | | |
| `batch_size` | `int` | The maximum size of a batch before it is flushed.<br />`default: 1049000` `unit: bytes` |
| `batch_timeout` | `int` | The maximum age of a batch before it is flushed.<br />`default: 1` `unit: bytes` |
| **OPTIONAL** - Requests | | |
| `encoding` | `string` | The encoding format used to serialize the events before flushing.<br />`no default` `enum: "ndjson", "text"` |
| `rate_limit_duration` | `int` | The window used for the `request_rate_limit_num` option<br />`default: 1` `unit: seconds` |
| `rate_limit_num` | `int` | The maximum number of requests allowed within the `rate_limit_duration` window.<br />`default: 10` |
| `request_in_flight_limit` | `int` | The maximum number of in-flight requests allowed at any given time.<br />`default: 10` |
| `request_timeout_secs` | `int` | The maximum time a request can take before being aborted.<br />`default: 60` `unit: seconds` |
| `retry_attempts` | `int` | The maximum number of retries to make for failed requests.<br />`default: 5` |
| `retry_backoff_secs` | `int` | The amount of time to wait before attempting a failed request again.<br />`default: 5` `unit: seconds` |
| **OPTIONAL** - Buffer | | |
| `type` | `string` | The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.<br />`default: "memory"` `enum: "memory", "disk"` |
| `when_full` | `string` | The behavior when the buffer becomes full.<br />`default: "block"` `enum: "block", "drop_newest"` |
| `max_size` | `int` | Only relevant when `type` is `disk`. The maximum size of the buffer on the disk.<br />`no default` `example: 104900000` |
| `num_items` | `int` | Only relevant when `type` is `memory`. The maximum number of [events][docs.event] allowed in the buffer.<br />`default: 500` |

## How It Works

### Buffering, Batching, & Partitioning

![][images.sink-flow-partitioned]

The `splunk_hec` sink buffers, batches, and
partitions data as shown in the diagram above. You'll notice that Vector treats
these concepts differently, instead of treating them as global concepts, Vector
treats them as sink specific concepts. This isolates sinks, ensuring services
disruptions are contained and [delivery guarantees][docs.guarantees] are
honored.

#### Buffers types

The `buffer.type` option allows you to control buffer resource usage:

| Type     | Description                                                                                                    |
|:---------|:---------------------------------------------------------------------------------------------------------------|
| `memory` | Pros: Fast. Cons: Not persisted across restarts. Possible data loss in the event of a crash. Uses more memory. |
| `disk`   | Pros: Persisted across restarts, durable. Uses much less memory. Cons: Slower, see below.                      |

#### Buffer overflow

The `buffer.when_full` option allows you to control the behavior when the
buffer overflows:

| Type          | Description                                                                                                                        |
|:--------------|:-----------------------------------------------------------------------------------------------------------------------------------|
| `block`       | Applies back pressure until the buffer makes room. This will help to prevent data loss but will cause data to pile up on the edge. |
| `drop_newest` | Drops new data as it's received. This data is lost. This should be used when performance is the highest priority.                  |

#### Batch flushing

Batches are flushed when 1 of 2 conditions are met:

1. The batch age meets or exceeds the configured `batch_timeout` (default: `1 bytes`).
2. The batch size meets or exceeds the configured `batch_size` (default: `1049000 bytes`).

#### Partitioning

Partitioning is controlled via the 
options and allows you to dynamically partition data. You'll notice that
[`strftime` specifiers][url.strftime_specifiers] are allowed in the values,
enabling dynamic partitioning. The interpolated result is effectively the
internal batch partition key. Let's look at a few examples:

| Value | Interpolation | Desc |
|:------|:--------------|:-----|
| `date=%F` | `date=2019-05-02` | Partitions data by the event's day. |
| `date=%Y` | `date=2019` | Partitions data by the event's year. |
| `timestamp=%s` | `timestamp=1562450045` | Partitions data by the unix timestamp. |

### Delivery Guarantee

This component offers an [**at least once** delivery guarantee][docs.at_least_once_delivery]
if your [pipeline is configured to achieve this][docs.at_least_once_delivery].

### Encodings

The `splunk_hec` sink encodes events before writing
them downstream. This is controlled via the `encoding` option which accepts
the following options:

| Encoding | Description |
| :------- | :---------- |
| `ndjson` | The payload will be encoded in new line delimited JSON payload, each line representing a JSON encoded event. |
| `text` | The payload will be encoded as new line delimited text, each line representing the value of the `"message"` key. |

### Health Checks

Upon [starting][docs.starting], Vector will perform a simple health check
against this sink. The ensures that the downstream service is healthy and
reachable. By default, if the health check fails an error will be logged and
Vector will proceed to restart. Vector will continually check the health of
the service on an interval until healthy.

If you'd like to exit immediately when a service is unhealthy you can pass
the `--require-healthy` flag:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

Be careful when doing this if you have multiple sinks configured, as it will
prevent Vector from starting is one sink is unhealthy, preventing the other
healthy sinks from receiving data.

### Rate Limiting

Vector offers a few levers to control the rate and volume of requests to the
downstream service. Start with the `rate_limit_duration` and `rate_limit_num`
options to ensure Vector does not exceed the specified number of requests in
the specified window. You can further control the pace at which this window is
saturated with the `request_in_flight_limit` option, which will guarantee no
more than the specified number of requests are in-flight at any given time.

Please note, Vector's defaults are carefully chosen and it should be rare that
you need to adjust these. If you found a good reason to do so please share it
with the Vector team by [opening an issie][url.new_splunk_hec_sink_issue].

### Retry Policy

Vector will retry failed requests (status == `429`, >= `500`, and != `501`).
Other responses will _not_ be retried. You can control the number of retry
attempts and backoff rate with the `retry_attempts` and `retry_backoff_secs` options.

### Timeouts

To ensure the pipeline does not halt when a service fails to respond Vector
will abort requests after `60 seconds`.
This can be adjsuted with the `request_timeout_secs` option.

It is highly recommended that you do not lower value below the service's
internal timeout, as this could create orphaned requests, pile on retries,
and result in deuplicate data downstream.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.splunk_hec_sink_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.

### Setup

In order to supply values for both the `host` and `token` options you must first
setup a Splunk HTTP Event Collector. Please refer to the [Splunk setup
docs][url.splunk_hec_setup] for a guide on how to do this. Once you've setup
your Spunk HTTP Collectory you'll be provided a `host` and `token` that you
should supply to the `host` and `token` options.

## Resources

* [**Issues**][url.splunk_hec_sink_issues] - [enhancements][url.splunk_hec_sink_enhancements] - [bugs][url.splunk_hec_sink_bugs]
* [**Source code**][url.splunk_hec_sink_source]


[docs.at_least_once_delivery]: ../../../about/guarantees.md#at-least-once-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.event]: ../../../about/data-model.md#event
[docs.guarantees]: ../../../about/guarantees.md
[docs.log_event]: ../../../about/data-model.md#log
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.starting]: ../../../usage/administration/starting.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.sink-flow-partitioned]: ../../../assets/sink-flow-partitioned.svg
[images.splunk_hec_sink]: ../../../assets/splunk_hec-sink.svg
[url.community]: https://vector.dev/community
[url.new_splunk_hec_sink_issue]: https://github.com/timberio/vector/issues/new?labels%5B%5D=Sink%3A+splunk_hec
[url.search_forum]: https://forum.vector.dev/search?expanded=true
[url.splunk_hec]: http://dev.splunk.com/view/event-collector/SP-CAAAE6M
[url.splunk_hec_setup]: https://docs.splunk.com/Documentation/Splunk/latest/Data/UsetheHTTPEventCollector
[url.splunk_hec_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+splunk_hec%22+label%3A%22Type%3A+Bugs%22
[url.splunk_hec_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+splunk_hec%22+label%3A%22Type%3A+Enhancements%22
[url.splunk_hec_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+splunk_hec%22
[url.splunk_hec_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/splunk_hec.rs
[url.strftime_specifiers]: https://docs.rs/chrono/0.3.1/chrono/format/strftime/index.html
