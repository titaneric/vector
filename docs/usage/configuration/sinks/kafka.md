---
description: Streams `log` events to Apache Kafka via the Kafka protocol.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/kafka.md.erb
-->

# kafka sink

![][images.kafka_sink]


The `kafka` sink streams [`log`][docs.log_event] events to [Apache Kafka][url.kafka] via the [Kafka protocol][url.kafka_protocol].

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_kafka_sink_id]
  # REQUIRED - General
  type = "kafka" # must be: "kafka"
  inputs = ["my-source-id"]
  bootstrap_servers = "10.14.22.123:9092,10.14.23.332:9092"
  topic = "topic-1234"

  # OPTIONAL - General
  encoding = "json" # no default, enum: "json", "text"
  key_field = "partition_key" # no default

  # OPTIONAL - Buffer
  [sinks.my_kafka_sink_id.buffer]
    type = "memory" # default, enum: "memory", "disk"
    when_full = "block" # default, enum: "block", "drop_newest"
    max_size = 104900000 # no default
    num_items = 500 # default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  # REQUIRED - General
  type = "kafka"
  inputs = ["<string>", ...]
  bootstrap_servers = "<string>"
  topic = "<string>"

  # OPTIONAL - General
  encoding = {"json" | "text"}
  key_field = "<string>"

  # OPTIONAL - Buffer
  [sinks.<sink-id>.buffer]
    type = {"memory" | "disk"}
    when_full = {"block" | "drop_newest"}
    max_size = <int>
    num_items = <int>
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```coffeescript
[sinks.kafka]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "kafka"
  type = "kafka"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # A comma-separated list of host and port pairs that are the addresses of the
  # Kafka brokers in a "bootstrap" Kafka cluster that a Kafka client connects to
  # initially to bootstrap itself
  # 
  # * required
  # * no default
  bootstrap_servers = "10.14.22.123:9092,10.14.23.332:9092"

  # The Kafka topic name to write events to.
  # 
  # * required
  # * no default
  topic = "topic-1234"

  # The encoding format used to serialize the events before flushing.
  # 
  # * optional
  # * no default
  # * enum: "json", "text"
  encoding = "json"
  encoding = "text"

  # The field name to use for the topic key. If unspecified, the key will be
  # randomly generated. If the field does not exist on the event, a blank value
  # will be used.
  # 
  # * optional
  # * no default
  key_field = "partition_key"

  #
  # Buffer
  #

  [sinks.kafka.buffer]
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
| `type` | `string` | The component type<br />`required` `enum: "kafka"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `bootstrap_servers` | `string` | A comma-separated list of host and port pairs that are the addresses of the Kafka brokers in a "bootstrap" Kafka cluster that a Kafka client connects to initially to bootstrap itself<br />`required` `example: (see above)` |
| `topic` | `string` | The Kafka topic name to write events to.<br />`required` `example: "topic-1234"` |
| **OPTIONAL** - General | | |
| `encoding` | `string` | The encoding format used to serialize the events before flushing.<br />`no default` `enum: "json", "text"` |
| `key_field` | `string` | The field name to use for the topic key. If unspecified, the key will be randomly generated. If the field does not exist on the event, a blank value will be used.<br />`no default` `example: "partition_key"` |
| **OPTIONAL** - Buffer | | |
| `type` | `string` | The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.<br />`default: "memory"` `enum: "memory", "disk"` |
| `when_full` | `string` | The behavior when the buffer becomes full.<br />`default: "block"` `enum: "block", "drop_newest"` |
| `max_size` | `int` | Only relevant when `type` is `disk`. The maximum size of the buffer on the disk.<br />`no default` `example: 104900000` |
| `num_items` | `int` | Only relevant when `type` is `memory`. The maximum number of [events][docs.event] allowed in the buffer.<br />`default: 500` |

## How It Works

### Delivery Guarantee

This component offers an [**at least once** delivery guarantee][docs.at_least_once_delivery]
if your [pipeline is configured to achieve this][docs.at_least_once_delivery].

### Encodings

The `kafka` sink encodes events before writing
them downstream. This is controlled via the `encoding` option which accepts
the following options:

| Encoding | Description |
| :------- | :---------- |
| `json` | The payload will be encoded as a single JSON payload. |
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

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.kafka_sink_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.

## Resources

* [**Issues**][url.kafka_sink_issues] - [enhancements][url.kafka_sink_enhancements] - [bugs][url.kafka_sink_bugs]
* [**Source code**][url.kafka_sink_source]


[docs.at_least_once_delivery]: ../../../about/guarantees.md#at-least-once-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.event]: ../../../about/data-model.md#event
[docs.log_event]: ../../../about/data-model.md#log
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.starting]: ../../../usage/administration/starting.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.kafka_sink]: ../../../assets/kafka-sink.svg
[url.community]: https://vector.dev/community
[url.kafka]: https://kafka.apache.org/
[url.kafka_protocol]: https://kafka.apache.org/protocol
[url.kafka_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+kafka%22+label%3A%22Type%3A+Bugs%22
[url.kafka_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+kafka%22+label%3A%22Type%3A+Enhancements%22
[url.kafka_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+kafka%22
[url.kafka_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/kafka.rs
[url.search_forum]: https://forum.vector.dev/search?expanded=true
