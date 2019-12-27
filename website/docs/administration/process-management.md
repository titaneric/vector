---
title: Process Management
description: "How to manage the Vector process, such as starting, stopping, and reloading."
---

This document covers how to manage the Vector process using various process
managers.

## Starting

Vector can be started by calling the `vector` binary directly, no subcommand is
necessary.

import Tabs from '@theme/Tabs';

<Tabs
  block={true}
  defaultValue="manual"
  values={[
    { label: 'Manual', value: 'manual', },
    { label: 'Sytemd', value: 'systemd', },
    { label: 'Initd', value: 'initd', },
    { label: 'Homebrew', value: 'homebrew', },
  ]
}>

import TabItem from '@theme/TabItem';

<TabItem value="manual">

```bash
vector --config /etc/vector/vector.toml
```

</TabItem>
<TabItem value="systemd">

```bash
sudo systemctl start vector
```

</TabItem>
<TabItem value="initd">

```bash
/etc/init.d/vector start
```

</TabItem>
<TabItem value="homebrew">

```bash
brew services start vector
```

</TabItem>
</Tabs>

### Flags

import Fields from '@site/src/components/Fields';

import Field from '@site/src/components/Field';

<Fields>
<Field
  common={true}
  defaultValue={"/etc/vector/vector.toml"}
  name={"-c, --config"}
  nullable={false}
  required={true}
  type={"string"}>

#### -c, --config

Path to the Vector [configuration file][docs.configuration].

</Field>

<Field
  common={true}
  defaultValue={"/etc/vector/vector.toml"}
  name={"-c, --config"}
  nullable={false}
  required={true}
  type={"string"}>

#### test

test

</Field>
</Fields>


| Flag | Description |
| :--- | :--- |
| **Required** |  |  |
| `-c, --config <path>` | Path the Vector [configuration file][docs.configuration]. |
| **Optional** |  |  |
| `-d, --dry-run` | Vector will [validate configuration][docs.validating] and exit. |
| `-q, --quiet` | Raises the log level to `warn`. |
| `-qq` | Raises the log level to `error`, the highest level possible. |
| `-r, --require-healthy` | Causes vector to immediately exit if any sinks fail their healthchecks. |
| `-t, --threads` | Limits the number of internal threads Vector can spawn. See the [Limiting Resources][docs.roles.agent#limiting-resources] in the [Agent role][docs.roles.agent] documentation. |
| `-v, --verbose` | Drops the log level to `debug`. |
| `-vv` | Drops the log level to `trace`, the lowest level possible. |

### Daemonizing

Vector does not _directly_ offer a way to daemonize the Vector process. We
highly recommend that you use a utility like [Systemd][urls.systemd] to
daemonize and manage your processes. Vector provides a
[`vector.service` file][urls.vector_systemd_file] for Systemd.

## Stopping

The Vector process should be stopped by sending it a `SIGTERM` process signal:

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

<TabItem value="manual">

```bash
kill -SIGTERM <vector-process-id>
```

</TabItem>
<TabItem value="systemd">

```bash
sudo systemctl stop vector
```

</TabItem>
<TabItem value="initd">

```bash
/etc/init.d/vector stop
```

</TabItem>
<TabItem value="homebrew">

```bash
brew services stop vector
```

</TabItem>
</Tabs>

If you are currently running the Vector process in your terminal, this can be
achieved by a single `ctrl+c` key combination.

### Graceful Shutdown

Vector is designed to gracefully shutdown within 20 seconds when a `SIGTERM`
process signal is received. The shutdown process is as follows:

1. Stop accepting new data for all [sources][docs.sources].
2. Gracefully close any open connections with a 20 second timeout.
3. Flush any sink buffers with a 20 second timeout.
4. Exit the process with a 1 code.

### Force Killing

If Vector is forcefully killed there is potential for losing any in-flight
data. To mitigate this we recommend enabling on-disk buffers and avoiding
forceful shutdowns whenever possible.

### Exit Codes

If Vector fails to start it will exit with one of the preferred exit codes
as defined by `sysexits.h`. A full list of exit codes can be found in the
[`exitcodes` Rust crate][urls.exit_codes]. The relevant codes that Vector uses
are:

| Code | Description |
|:-----|:------------|
| `0`  | No error. |
| `78` | Bad [configuration][docs.configuration]. |

## Reloading

Vector can be reloaded, on the fly, to recognize any configuration changes by
sending the Vector process a `SIGHUP` signal:

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

<TabItem value="manual">

```bash
kill -SIGHUP <vector-process-id>
```

You can find the Vector process ID with:

```bash
ps -ax vector | grep vector
```

</TabItem>
<TabItem value="systemd">

```bash
systemctl kill -s HUP --kill-who=main vector.service
```

</TabItem>
<TabItem value="initd">

```bash
/etc/init.d/vector reload
```

</TabItem>
<TabItem value="homebrew">

```bash
kill -SIGHUP <vector-process-id>
```

You can find the Vector process ID with:

```bash
ps -ax vector | grep vector
```

</TabItem>
</Tabs>

### Configuration Errors

When Vector is reloaded it proceeds to read the new configuration file from
disk. If the file has errors it will be logged to `STDOUT` and ignored,
preserving any previous configuration that was set. If the process exits you
will not be able to restart the process since it will proceed to use the
new configuration file.

### Graceful Pipeline Transitioning

Vector will perform a diff between the new and old configuration, determining
which sinks and sources should be started and shutdown and ensures the
transition from the old to new pipeline is graceful.


[docs.configuration]: /docs/setup/configuration/
[docs.roles.agent#limiting-resources]: /docs/setup/deployment/roles/agent/#limiting-resources
[docs.roles.agent]: /docs/setup/deployment/roles/agent/
[docs.sources]: /docs/reference/sources/
[docs.validating]: /docs/administration/validating/
[urls.exit_codes]: https://docs.rs/exitcode/1.1.2/exitcode/#constants
[urls.systemd]: https://www.freedesktop.org/wiki/Software/systemd/
[urls.vector_systemd_file]: https://github.com/timberio/vector/blob/master/distribution/systemd/vector.service
