---
description: Exposes `metric` events to Prometheus metrics service.
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/prometheus.md.erb
-->

# prometheus sink

![][images.prometheus_sink]

{% hint style="warning" %}
The `prometheus` sink is in beta. Please see the current
[enhancements][url.prometheus_sink_enhancements] and
[bugs][url.prometheus_sink_bugs] for known issues.
We kindly ask that you [add any missing issues][url.new_prometheus_sink_issue]
as it will help shape the roadmap of this component.
{% endhint %}

The `prometheus` sink [exposes](#exposing-and-scraping) [`metric`][docs.metric_event] events to [Prometheus][url.prometheus] metrics service.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```coffeescript
[sinks.my_prometheus_sink_id]
  # REQUIRED - General
  type = "prometheus" # must be: "prometheus"
  inputs = ["my-source-id"]
  address = "0.0.0.0:9598"
  
  # OPTIONAL - Buffer
  [sinks.my_prometheus_sink_id.buffer]
    type = "memory" # default, enum: "memory" or "disk"
    when_full = "block" # default, enum: "block" or "drop_newest"
    max_size = 104900000 # no default, bytes, relevant when type = "disk"
    num_items = 500 # default, events, relevant when type = "memory"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```coffeescript
[sinks.<sink-id>]
  # REQUIRED - General
  type = "prometheus"
  inputs = ["<string>", ...]
  address = "<string>"

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
[sinks.prometheus_sink]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "prometheus"
  type = "prometheus"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # The address to expose for scraping.
  # 
  # * required
  # * no default
  address = "0.0.0.0:9598"

  #
  # Buffer
  #

  [sinks.prometheus_sink.buffer]
    # The buffer's type / location. `disk` buffers are persistent and will be
    # retained between restarts.
    # 
    # * optional
    # * default: "memory"
    # * enum: "memory" or "disk"
    type = "memory"
    type = "disk"

    # The behavior when the buffer becomes full.
    # 
    # * optional
    # * default: "block"
    # * enum: "block" or "drop_newest"
    when_full = "block"
    when_full = "drop_newest"

    # The maximum size of the buffer on the disk.
    # 
    # * optional
    # * no default
    # * unit: bytes
    max_size = 104900000

    # The maximum number of events allowed in the buffer.
    # 
    # * optional
    # * default: 500
    # * unit: events
    num_items = 500
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** - General | | |
| `type` | `string` | The component type<br />`required` `must be: "prometheus"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| `address` | `string` | The address to expose for scraping. See [Exposing & Scraping](#exposing-scraping) for more info.<br />`required` `example: "0.0.0.0:9598"` |
| **OPTIONAL** - Buffer | | |
| `buffer.type` | `string` | The buffer's type / location. `disk` buffers are persistent and will be retained between restarts.<br />`default: "memory"` `enum: "memory" or "disk"` |
| `buffer.when_full` | `string` | The behavior when the buffer becomes full.<br />`default: "block"` `enum: "block" or "drop_newest"` |
| `buffer.max_size` | `int` | The maximum size of the buffer on the disk. Only relevant when type = "disk"<br />`no default` `example: 104900000` `unit: bytes` |
| `buffer.num_items` | `int` | The maximum number of [events][docs.event] allowed in the buffer. Only relevant when type = "memory"<br />`default: 500` `unit: events` |

## How It Works

### Delivery Guarantee

This component offers an [**at least once** delivery guarantee][docs.at_least_once_delivery]
if your [pipeline is configured to achieve this][docs.at_least_once_delivery].

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration.environment-variables]
section.

### Exposing & Scraping

The `prometheus` sink exposes data for scraping.
The `address` option determines the address and port the data is made available
on. You'll need to configure your networking so that the configured port is
accessible by the downstream service doing the scraping.

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

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `prometheus_sink` issues][url.prometheus_sink_issues].
2. If encountered a bug, please [file a bug report][url.new_prometheus_sink_bug].
3. If encountered a missing feature, please [file a feature request][url.new_prometheus_sink_enhancement].
4. If you need help, [join our chat/forum community][url.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][url.prometheus_sink_issues] - [enhancements][url.prometheus_sink_enhancements] - [bugs][url.prometheus_sink_bugs]
* [**Source code**][url.prometheus_sink_source]


[docs.at_least_once_delivery]: ../../../about/guarantees.md#at-least-once-delivery
[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.configuration.environment-variables]: ../../../usage/configuration#environment-variables
[docs.event]: ../../../about/data-model/README.md#event
[docs.metric_event]: ../../../about/data-model/metric.md
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.starting]: ../../../usage/administration/starting.md
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.prometheus_sink]: ../../../assets/prometheus-sink.svg
[url.new_prometheus_sink_bug]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+prometheus&labels=Type%3A+Bug
[url.new_prometheus_sink_enhancement]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+prometheus&labels=Type%3A+Enhancement
[url.new_prometheus_sink_issue]: https://github.com/timberio/vector/issues/new?labels=Sink%3A+prometheus
[url.prometheus]: https://prometheus.io/
[url.prometheus_sink_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+prometheus%22+label%3A%22Type%3A+Bug%22
[url.prometheus_sink_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+prometheus%22+label%3A%22Type%3A+Enhancement%22
[url.prometheus_sink_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Sink%3A+prometheus%22
[url.prometheus_sink_source]: https://github.com/timberio/vector/tree/master/src/sinks/prometheus.rs
[url.vector_chat]: https://chat.vector.dev
