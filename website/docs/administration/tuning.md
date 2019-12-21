---
title: Tuning
description: How to tune Vector.
---

This document will cover tuning Vector.

## Maximizing Resources

Vector is written in [Rust][urls.rust] and therefore does not include a runtime
or VM. There are no special service level steps you need to do to improve
performance. By default, Vector will take full advantage of all system
resources.

## Limiting Resources

Conversely, when deploying Vector in the [agent role][docs.roles.agent] you'll
typically want to limit resources:

import Jump from '@site/src/components/Jump';

<Jump to="/docs/setup/deployment/roles/agent#limiting-resources">View limiting resources section</Jump>


[docs.roles.agent]: /docs/setup/deployment/roles/agent/
[urls.rust]: https://www.rust-lang.org/
