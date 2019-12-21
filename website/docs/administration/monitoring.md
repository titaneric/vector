---
title: Monitoring
description: How to monitor and observe Vector with logs, metrics, and more.
---

This document will cover monitoring Vector.

## Logs

Vector writes all output to `STDOUT`, therefore, you have complete control of
the output destination. Accessing the logs depends on your service manager:

import Tabs from '@theme/Tabs';

<Tabs
  block={true}
  defaultValue="manual"
  values={[
    { label: 'Manual', value: 'manual', },
    { label: 'Systemd', value: 'systemd', },
    { label: 'Initd', value: 'initd', },
    { label: 'Homebrew', value: 'homebrew', },
  ]
}>

import TabItem from '@theme/TabItem';

<TabItem value="manual">

```bash
tail /var/log/vector.log
```

</TabItem>
<TabItem value="systemd">

```bash
sudo journalctl -fu vector
```

</TabItem>
<TabItem value="initd">

```bash
tail -f /var/log/vector.log
```

</TabItem>
<TabItem value="homebrew">

```bash
tail -f /usr/local/var/log/vector.log
```

</TabItem>
</Tabs>

### Levels

By default, Vector logs on the `info` level, you can change the level through
a variety of methods:

| Method | Description |
| :----- | :---------- |
| [`-v` flag][docs.process-management#flags] | Drops the log level to `debug`. |
| [`-vv` flag][docs.process-management#flags] | Drops the log level to `trace`. |
| [`-q` flag][docs.process-management#flags] | Raises the log level to `warn`. |
| [`-qq` flag][docs.process-management#flags] | Raises the log level to `error`. |
| `LOG=<level>` env var | Set the log level. Must be one of `trace`, `debug`, `info`, `warn`, `error`. |

### Full Backtraces

You can enable full error backtraces by setting the  `RUST_BACKTRACE=full` env
var. More on this in the [Troubleshooting guide][docs.troubleshooting].

### Rate Limiting

Vector rate limits log events in the hot path. This is to your benefit as
it allows you to get granular insight without the risk of saturating IO
and disrupting service. The tradeoff is that repetitive logs will not be logged.

## Metrics

Currently, Vector does not expose Metrics. [Issue #230][urls.issue_230]
represents work to run internal Vector metrics through Vector's pipeline.
Allowing you to define internal metrics as a [source][docs.sources] and
then define one of many metrics [sinks][docs.sinks] to collect those metrics,
just as you would metrics from any other source.

## Troubleshooting

Please refer to our troubleshooting guide:

import Jump from '@site/src/components/Jump';

<Jump to="/docs/setup/guides/troubleshooting">Troubleshooting Guide</Jump>


[docs.process-management#flags]: /docs/administration/process-management/#flags
[docs.sinks]: /docs/reference/sinks/
[docs.sources]: /docs/reference/sources/
[docs.troubleshooting]: /docs/setup/guides/troubleshooting/
[urls.issue_230]: https://github.com/timberio/vector/issues/230
