<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/transforms/json_parser.md.erb
-->

---
description: Accepts `log` events and allows you to parse a field value as JSON.
---

# json_parser transform

![][images.json_parser_transform]


The `json_parser` transform accepts [`log`][docs.log_event] events and allows you to parse a field value as JSON.

## Config File

{% code-tabs %}
{% code-tabs-item title="vector.toml (example)" %}
```toml
[sinks.my_json_parser_transform_id]
  # REQUIRED
  type = "json_parser" # must be: "json_parser"
  inputs = ["my-source-id"]

  # OPTIONAL
  drop_invalid = true # no default
  field = "message" # default
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (schema)" %}
```toml
[sinks.<sink-id>]
  # REQUIRED
  type = "json_parser"
  inputs = ["<string>", ...]

  # OPTIONAL
  drop_invalid = <bool>
  field = "<string>"
```
{% endcode-tabs-item %}
{% code-tabs-item title="vector.toml (specification)" %}
```toml
[sinks.json_parser]
  #
  # General
  #

  # The component type
  # 
  # * required
  # * no default
  # * must be: "json_parser"
  type = "json_parser"

  # A list of upstream source or transform IDs. See Config Composition for more
  # info.
  # 
  # * required
  # * no default
  inputs = ["my-source-id"]

  # If `true` events with invalid JSON will be dropped, otherwise the event will
  # be kept and passed through.
  # 
  # * optional
  # * no default
  drop_invalid = true

  # The field decode as JSON. Must be a `string` value.
  # 
  # * optional
  # * default: "message"
  field = "message"
```
{% endcode-tabs-item %}
{% endcode-tabs %}

## Options

| Key  | Type  | Description |
|:-----|:-----:|:------------|
| **REQUIRED** | | |
| `type` | `string` | The component type<br />`required` `enum: "json_parser"` |
| `inputs` | `[string]` | A list of upstream [source][docs.sources] or [transform][docs.transforms] IDs. See [Config Composition][docs.config_composition] for more info.<br />`required` `example: ["my-source-id"]` |
| **OPTIONAL** | | |
| `drop_invalid` | `bool` | If `true` events with invalid JSON will be dropped, otherwise the event will be kept and passed through.<br />`no default` `example: true` |
| `field` | `string` | The field decode as JSON. Must be a `string` value.<br />`default: "message"` |

## Examples

{% tabs %}
{% tab title="Simple" %}
Given the following log event:

```
{
  "message": "{"key": "value"}"
}
```

You can parse the JSON with:

```coffeescript
[transforms.json]
  inputs = ["<source_id>"]
  type   = "json_parser"
  field  = "message"
```

This would produce the following event as output:

```javascript
{
  "key": "value"
}
```

By default, Vector drops fields after parsing them via the `drop_field`
option.

{% endtab %}
{% tab title="Wrapped" %}
It is possible to chain `json_parser` transforms to effectively "unwrap"
nested JSON documents. For example, give this log event:

```
{
  "message": "{"parent": "{\"child\": \"value2\"}"}"
}
```

You could unwrap the JSON with the following transforms:

```coffeescript
[transforms.root_json]
  inputs = ["<source_id>"]
  type   = "json_parser"
  field  = "message"

[transforms.parent_json]
  inputs = ["root_json"]
  type   = "json_parser"
  field  = "parent"

[transforms.child_json]
  inputs = ["parent_json"]
  type   = "json_parser"
  field  = "child"
```

This would produce the following event as output:

```javascript
{
  "child": "value2"
}
```

By default, Vector drops fields after parsing them via the `drop_field`
option.

{% endtab %}
{% endtabs %}

## How It Works



### Chaining / Unwrapping

Please see the [I/O section](#i-o) for an example of chaining and unwrapping JSON.

### Correctness

The `json_parser` source has been involved in the following correctness tests:

* [`wrapped_json_correctness`][url.wrapped_json_correctness_test]

Learn more in the [Correctness][docs.correctness] sections.

### Invalid JSON

If the value for the specified `field` is not valid JSON you can control keep or discard the event with the `drop_invalid` option. Setting it to `true` will discard the event and drop it entirely. Setting it to `false` will keep the event and pass it through. Note that passing through the event could cause problems and violate assumptions about the structure of your event.

### Key Conflicts

Any key present in the decoded JSON will override existin keys in the event.

### Nested Fields

If the decoded JSON includes nested fields it will be _deep_ merged into the event. For example, given the following event:

```javascript
{
  "message": "{"parent": {"child2": "value2"}}",
  "parent": {
    "child1": "value1"
  }
}
```

Parsing the `"message"` field would result the following structure:

```javascript
{
  "parent": {
    "child1": "value1",
    "child2": "value2"
  }
}
```

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring_logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open sink issues][url.json_parser_transform_issues].
2. [Search the forum][url.search_forum] for any similar issues.
2. Reach out to the [community][url.community] for help.

## Resources

* [**Issues**][url.json_parser_transform_issues] - [enhancements][url.json_parser_transform_enhancements] - [bugs][url.json_parser_transform_bugs]
* [**Source code**][url.json_parser_transform_source]


[docs.config_composition]: ../../../usage/configuration/README.md#composition
[docs.correctness]: ../../../correctness.md
[docs.log_event]: ../../../about/data-model.md#log
[docs.monitoring_logs]: ../../../usage/administration/monitoring.md#logs
[docs.sources]: ../../../usage/configuration/sources
[docs.transforms]: ../../../usage/configuration/transforms
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[images.json_parser_transform]: ../../../assets/json_parser-transform.svg
[url.community]: https://vector.dev/community
[url.json_parser_transform_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+json_parser%22+label%3A%22Type%3A+Bugs%22
[url.json_parser_transform_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+json_parser%22+label%3A%22Type%3A+Enhancements%22
[url.json_parser_transform_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22Transform%3A+json_parser%22
[url.json_parser_transform_source]: https://github.com/timberio/vector/tree/master/src/transforms/json_parser.rs
[url.search_forum]: https://forum.vector.dev/search?expanded=true
[url.wrapped_json_correctness_test]: https://github.com/timberio/vector-test-harness/tree/master/cases/wrapped_json_correctness
