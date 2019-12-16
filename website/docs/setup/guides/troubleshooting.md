---
title: Troubleshooting Guide
sidebar_label: Troubleshooting
description: A guide on debugging and troubleshooting Vector
---

This guide covers troubleshooting Vector. The sections are intended to be
followed in order.

First, we're sorry to hear that you are having trouble with Vector. Reliability
and operator friendliness are _very_ important to us, and we urge you to
[open an issue][urls.new_bug_report] to let us know what's going on. This helps
us improve Vector.

## 1. Check for any known issues

Start by searching [Vector's issues][urls.vector_issues]. You can filter
to the specific component via the `lable` filter.

## 2. Check Vector's logs

We've taken great care to ensure Vector's logs are high-quality and helpful.
In most cases the logs will surface the issue:

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

If you are not using a service manager, and you're redirecting Vector's
output to a file then you can use a utility like `tail` to access your logs:

```bash
tail /var/log/vector.log
```

</TabItem>
<TabItem value="systemd">

Tail logs:

```bash
sudo journalctl -fu vector
```

</TabItem>
<TabItem value="initd">

Tail logs:

```bash
tail -f /var/log/vector.log
```

</TabItem>
<TabItem value="homebrew">

Tail logs:

```bash
tail -f /usr/local/var/log/vector.log
```

</TabItem>
</Tabs>

## 3. Enable backtraces

import Alert from '@site/src/components/Alert';

<Alert type="info">

You can skip to the [next section](#3-enable-debug-logging) if you do not

</Alert>

If you see an exception in Vector's logs then we've clearly found the issue.
Before you report a bug, please enable backtraces:

```bash
RUST_BACKTRACE=full vector --config=/etc/vector/vector.toml
```

Backtraces are _critical_ for debugging errors. Once you have the backtrace
please [open a bug report issue][urls.new_bug_report].

## 4. Enable debug logging

If you do not see an error in your Vector logs, and the Vector logs appear
to be frozen, then you'll want to drop your log level to `debug`:

<Alert type="info">

Vector [rate limits][docs.monitoring#rate-limiting] logs in the hot path.
As a result, dropping to the `debug` level is safe for production environments.

</Alert>

<Tabs
  block={true}
  defaultValue="env_var"
  values={[
    { label: 'Env Var', value: 'env_var', },
    { label: 'Flag', value: 'flag', },
  ]
}>

<TabItem value="env_var">

```bash
LOG=debug vector --config=/etc/vector/vector.toml
```

</TabItem>
<TabItem value="flag">

```bash
vector --verbose --config=/etc/vector/vector.toml
```

</TabItem>
</Tabs>

## 5. Get help

At this point we recommend reaching out to the community for help.

1. If encountered a bug, please [file a bug report][urls.new_bug_report]
2. If encountered a missing feature, please [file a feature request][urls.new_feature_request].
3. If you need help, [join our chat community][urls.vector_chat]. You can post a question and search previous questions.


[docs.monitoring#rate-limiting]: /docs/administration/monitoring/#rate-limiting
[urls.new_bug_report]: https://github.com/timberio/vector/issues/new?labels=type%3A+bug
[urls.new_feature_request]: https://github.com/timberio/vector/issues/new?labels=type%3A+new+feature
[urls.vector_chat]: https://chat.vector.dev
[urls.vector_issues]: https://github.com/timberio/vector/issues
