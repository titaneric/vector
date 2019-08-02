---
description: Streams `log` and `metric` events to the console, `STDOUT` or `STDERR`.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/console.md.erb
-->

# console sink

![][images.console_sink]


The `console` sink [streams](#streaming) [`log`][docs.log_event] and [`metric`][docs.metric_event] events to the console, `STDOUT` or `STDERR`.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_console_sink_id]
  # REQUIRED - General
  type = "console" # must be: "console"
  inputs = ["my-source-id"]
  target = "stdout" # enum: "stdout", "stderr"
  
  # OPTIONAL - General
  encoding = "json" # default, enum: "json", "text"
  
  # OPTIONAL - Buffer
  [sinks.my_console_sink_id.buffer]
    type = "memory" # default, enum: "memory", "disk"
    when_full = "block" # default, enum: "block", "drop_newest"
    max_size = 104900000 # no default, bytes, relevant when type = "disk"
    num_items = 500 # default, events, relevant when type = "memory"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  # REQUIRED - General
  type = "console"
  inputs = ["<string>", ...]
  target = {"stdout" | "stderr"}

  # OPTIONAL - General
  encoding = {"json" | "text"}

  # OPTIONAL - Buffer
  [sinks.<sink-id>.buffer]
    type = {"memory" | "disk"}
    when_full = {"block" | "drop_newest"}
    max_size = <int>
    num_items = <int>
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `enum: "console"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `target` | `string` | The [standard stream][url.standard_streams] to write to.<br />`required` `enum: "stdout", "stderr"` |
| **OPTIONAL** - General | | |
| `encoding` | `string` | The encoding format used to serialize the events before writing. See [Encodings](#encodings) for more info.<br />`default: "dynamic"` `enum: "json", "text"` |
| **OPTIONAL** - Buffer | | |
| `buffer.type` | `string` | The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.<br />`default: "memory"` `enum: "memory", "disk"` |
| `buffer.when_full` | `string` | The behavior when the buffer becomes full.<br />`default: "block"` `enum: "block", "drop_newest"` |
| `buffer.max_size` | `int` | The maximum size of the buffer on the disk. Only relevant when type = "disk"<br />`no default` `example: 104900000` `unit: bytes` |
| `buffer.num_items` | `int` | The maximum number of [events][docs.event] allowed in the buffer. Only relevant when type = "memory"<br />`default: 500` `unit: events` |

## How It Works

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.best_effort_delivery].

### Encodings

The `console` sink encodes events before writing
them downstream. This is controlled via the `encoding` option which accepts
the following options:

| Encoding | Description |
| :------- | :---------- |
| `json` | The payload will be encoded as a single JSON payload. |
| `text` | The payload will be encoded as new line delimited text, each line representing the value of the `"message"` key. |

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Health Checks

Upon [starting][docs.starting], Vector will perform a simple health check
against this sink. The ensures that the downstream service is healthy and
reachable.
By default, if the health check fails an error will be logged and
Vector will proceed to start. If you'd like to exit immediately upomn healt
check failure, you can pass the `--require-healthy` flag:

```bash
vector --config /etc/vector/vector.toml --require-healthy
```

Be careful when doing this, one unhealthy sink can prevent other healthy sinks
from processing data at all.

### Streaming

The `console` sink streams data on a real-time
event-by-event basis. It does not batch data.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.console_sink_issues].
2. If encountered a bug, please [file a bug report][url.new_console_sink_bug].
3. If encountered a missing feature, please [file a feature request][url.new_console_sink_enhancement].
4. If you need help, [join our chat community][url.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][url.console_sink_issues] - [enhancements][url.console_sink_enhancements] - [bugs][url.console_sink_bugs]
* [**Source code**][url.console_sink_source]


[docs.best_effort_delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.event]: ../../../about/data-model/README.md#event
[docs.log_event]: ../../../about/data-model/log.md
[docs.metric_event]: ../../../about/data-model/metric.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.starting]: ../../../usage/administration/starting.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.console_sink]: ../../../assets/console-sink.svg
[url.console_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+console%22+label%3A%22Type%3A+Bug%22
[url.console_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+console%22+label%3A%22Type%3A+Enhancement%22
[url.console_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+console%22
[url.console_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/console.rs
[url.new_console_sink_bug]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+console&labels=Type%3A+Bug
[url.new_console_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+console&labels=Type%3A+Enhancement
[url.standard_streams]: https://en.wikipedia.org/wiki/Standard_streams
[url.vector_chat]: https://chat.vector.dev
