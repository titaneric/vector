# User Experience Design

Vector places high value on its user experience; our goal is to make this a
strong differentiator. But this presents a challenge for open-source products,
like Vector, where hundreds of contributors come together to build a common
product. As a result, definitions of good user experience differ, contributors
have varying levels of expertise in this area, and coordination suffers. To solve
this, we must align behind a list of [principles](#principles),
[guidelines](#guidelines), and [processes](#processes) that unify us towards a
shared vision of good user experience -- the purpose of this document.

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Pinciples](#pinciples)
   1. [Don't please everyone](#dont-please-everyone)
   1. [Be opinionated & reduce decisions](#be-opinionated--reduce-decisions)
   1. [Build momentum with consistency](#build-momentum-with-consistency)
1. [Guidelines](#guidelines)
   1. [Logical boundaries](#logical-boundaries)
      1. [Source & sink boundaries](#source--sink-boundaries)
      1. [Transform boundaries](#transform-boundaries)
   1. [Naming](#naming)
      1. [Option naming](#option-naming)
      1. [Metric naming](#metric-naming)
1. [Adherence](#adherence)
   1. [Roles](#roles)
      1. [Contributors](#contributors)
      1. [User experience committee](#user-experience-committee)
   1. [Responsibilities](#responsibilities)
      1. [Contributors](#contributors-1)
      1. [User experience committee](#user-experience-committee-1)

<!-- /MarkdownTOC -->

## Pinciples

### Don't please everyone

By nature of Vector's design, a data processing swiss army knife of sorts,
we often get presented with many different creative use cases -- using Vector
for analytics data, obscure IOT pipelines, or synchronizing state between
systems. It's tempting to entertain these use cases, especially if a large,
valuable user is inquiring about them, but we should refrain from doing so.
Trying to be everything to everyone means we'll never be great at anything.
Disparate use cases often have competing concerns that, when combined, result
in a lukewarm user experience. Vector is an _observability data pipeline_ and
we strive to be the best in this domain.

Examples:

- Avoiding analytics specific use cases.
- Leaning into tools like Kafka instead of trying to replace them.

### Be opinionated & reduce decisions

As an extension of the previous principle, aligning on specific use cases
allows us to make assumptions about the user and be opinionated with solutions.
This approach reduces the number of decisions a user has to make, a hallmark of
a good user experience. Therefore, as much as possible, we should offer
opinionated models for use cases core to Vector's purpose and not leave them as
creative exercises for the user.

Examples:

- Vector's pipeline model as a solution to team collaboration as opposed to
  generic config files.
- Vector's metric data model as a solution for metrics interoperability as
  opposed to specifically structured log lines.

### Build momentum with consistency

Consistency builds momentum by reducing the cognitive load imposed on a user
as they navigate a product. Users carry patterns from previous areas into new
ones; deviation from these patterns introduces unnecessary friction. Consistency
manifests itself in small details, like naming, and larger details, like product
behavior. For example, it would be surprising to find a `codec` option in one
source and a `decoder` option in another if they are functionally equivalent.
Even more surprising is a deviation in runtime behavior, like back pressure or
error handling, since this often results in data loss.

Examples:

- Using the same `codec` option name in both sources and sinks that support it.
- Defaulting to applying back pressure regardless of the component or topology.

## Guidelines

The following guidelines help to uphold the above principles.

### Logical boundaries

#### Source & sink boundaries

Vector sources and sinks should be plentiful and specific. When possible, they
should align with a well-defined protocol. If a well-defined protocol is not
available, or the use case does not allow it, then the source or sink should
align with its upstream client or target service. Additionally, sources and
sinks can overlap. It is preferable to offer specific sources and sinks composed
with broad sources and sinks. This promotes discoverability, aligns with user
intent, and builds confidence that Vector meets their specific use case.
Ideally, Vector would offer hundreds of sources and sinks that specifically
integrate with various systems. Maintainability should be achieved through
composability.

Examples:

- A `syslog` source as opposed to a `syslogng` source since it aligns with the
  Syslog protocol.
- A `datadog_agent` source as opposed to a `datadog_api` source since it aligns
  on intent and reduces scope.
- Again, a `syslog` source _in addition_ to a `socket` source with the `codec`
  option set to `syslog` since it is more specific and discoverable.

#### Transform boundaries

As opposed to source and sinks, Vector transforms should be broad and minimal.
They should align with a high-level use case, like mapping, aggregation, or
routing. The goal here is to reduce the number of choices a user makes and
be opinionated with solutions. Our opinions should prioritize performance and
reliability over ease of use (within reason), but we should strive to achieve
both.

Examples:

- A `remap` transform as opposed to multiple `parse_json`, `parse_syslog`, etc
  transforms.
- A `filter` transform as opposed to a `filter_regex`, `filter_datadog_search`,
  etc transforms.

### Naming

#### Option naming

Configuration option names should adhere to the following rules:

- Alphanumeric, lowercase, snake case format
- Use nouns, not verbs, as names (e.g., `fingerprint` instead of `fingerprinting`)
- Suffix options with their unit. (e.g., `_seconds`, `_bytes`, etc.)
- Be consistent with units within the same scope. (e.g., don't mix seconds and milliseconds)
- Don't repeat the name space in the option name (e.g., `fingerprint.bytes` instead of `fingerprint.fingerprint_bytes`)

#### Metric naming

For metric naming, Vector broadly follows the
[Prometheus metric naming standards](https://prometheus.io/docs/practices/naming/).
Hence, a metric name:

- Must only contain valid characters, which are ASCII letters and digits, as
  well as underscores. It should match the regular expression: `[a-z_][a-z0-9_]*`.
- Metrics have a broad template:

  `<namespace>_<name>_<unit>_[total]`

  - The `namespace` is a single word prefix that groups metrics from a specific
    source, for example host-based metrics like CPU, disk, and memory are
    prefixed with `host`, Apache metrics are prefixed with `apache`, etc.
  - The `name` describes what the metric measures.
  - The `unit` is a [single base unit](https://en.wikipedia.org/wiki/SI_base_unit),
    for example seconds, bytes, metrics.
  - The suffix should describe the unit in plural form: seconds, bytes.
    Accumulating counts, both with units or without, should end in `total`,
    for example `disk_written_bytes_total` and `http_requests_total`.

- Where required, use tags to differentiate the characteristic of the
  measurement. For example, whilst `host_cpu_seconds_total` is name of the
  metric, we also record the `mode` that is being used for each CPU. The `mode`
  and the specific CPU then become tags on the metric:

```text
host_cpu_seconds_total{cpu="0",mode="idle"}
host_cpu_seconds_total{cpu="0",mode="idle"}
host_cpu_seconds_total{cpu="0",mode="nice"}
host_cpu_seconds_total{cpu="0",mode="system"}
host_cpu_seconds_total{cpu="0",mode="user"}
host_cpu_seconds_total
```

## Adherence

### Roles

To keep things simple, only two roles are involved in the context of Vector's
user experience.

#### Contributors

Anyone contributing a change to Vector, both public open-source contributors
and internal Vector team members.

#### User experience committee

A select group of Vector team members responsible for Vector's resulting user
experience.

### Responsibilities

#### Contributors

As a Vector contributor you are responsible for coupling the following non-code
changes with your code changes:

* Reference docs changes located in the [`docs/cue` folder](docs/cue)
  (generally configuration changes)
* Existing guide changes located in the [`docs/content` folder](docs/content)
* If relevant, [highlighting] your change for future release notes

You are _not_ responsible for:

* Writing new guides related to your change

#### User experience committee

As a user experience design committee member, you are responsible for:

* The resulting user experience.
* Reviewing and approving proposed user experience changes in RFCs.
* Reviewing and approving user-facing changes in pull requests.
* Updating and evolving guides to reflect new Vector changes.
