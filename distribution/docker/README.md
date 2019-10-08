<p align="center">
  <strong>
    <a href="https://vector.dev">Website<a/>&nbsp;&nbsp;&bull;&nbsp;&nbsp;<a href="https://docs.vector.dev">Docs<a/>&nbsp;&nbsp;&bull;&nbsp;&nbsp;<a href="https://vector.dev/community">Community<a/>&nbsp;&nbsp;&bull;&nbsp;&nbsp;<a href="https://github.com/timberio/vector">Github<a/>
  </strong>
</p>

---

<p align="center">
  <img src="https://res.cloudinary.com/timber/image/upload/v1561214425/vector_diagram_w26yw3.svg" alt="Vector">
</p>

Vector is a [high-performance][docs.performance] observability data router. It
makes [collecting][docs.sources], [transforming][docs.transforms], and
[sending][docs.sinks] logs, metrics, and events easy. It decouples data
collection & routing from your services, giving you control and data ownership,
among [many other benefits][docs.use_cases].

Built in [Rust][urls.rust], Vector places high-value on
[performance][docs.performance], [correctness][docs.correctness], and [operator
friendliness][docs.administration]. It compiles to a single static binary and is
designed to be [deployed][docs.deployment] across your entire infrastructure,
serving both as a light-weight [agent][docs.roles.agent] and a highly efficient
[service][docs.roles.service], making the process of getting data from A to B
simple and unified.

## Usage

```bash
docker run timberio/vector:latest-alpine
```

* The `vector` binary is located at `/usr/local/bin/vector`, which should be in your `$PATH`.
* The default [configuration file][docs.configuration] is located at `/etc/vector/vector.toml`.

## Configuring

The Vector Docker images ship with a [default `/etc/vector/vector.toml` configuration file][urls.default_configuration].
To use your own configuration file:

1. Create your own [Vector configuration file][docs.configuration] and save it
   as `vector.toml`.

2. Run the Vector Docker image with the following command:

   ```bash
   docker run -v $PWD/vector.toml:/etc/vector/vector.toml:ro timberio/vector:latest-alpine
   ```

   Modify `$PWD` to the directory where you store your local `vector.toml` file.

## Image Variants

### timberio/vector:&lt;version&gt;-alpine

This image is based on the [`alpine` Docker image][urls.docker_alpine], which is
a Linux distribution built around musl libc and BusyBox. It is considerably
smaller in size than other Docker images and statically links libraries. This
is the image we recommend due to it's small size and reliability.

### timberio/vector:&lt;version&gt;-debian

This image is based on the [`debian-slim` image][urls.docker_debian],
which is a smaller, more compact variant of the [`debian` image][urls.docker_debian].

## Versions

Vector's Docker images include a special `latest` version that will be updated
whenever Vector is [released][urls.vector_releases]. All other
[releases][urls.vector_releases] are available via the `X.X.X` tag:

```bash
docker run timberio/vector:X.X.X-alpine
```

## Updating

Simply run the with the `latest` tag:

```bash
docker run timberio/vector:latest-alpine
```

Or specify the exact version:

```bash
docker run timberio/vector:X.X.X-alpine
```


[docs.administration]: https://docs.vector.dev/usage/administration
[docs.configuration]: https://docs.vector.dev/usage/configuration
[docs.correctness]: https://docs.vector.dev/correctness
[docs.deployment]: https://docs.vector.dev/setup/deployment
[docs.performance]: https://docs.vector.dev/performance
[docs.roles.agent]: https://docs.vector.dev/setup/deployment/roles/agent
[docs.roles.service]: https://docs.vector.dev/setup/deployment/roles/service
[docs.sinks]: https://docs.vector.dev/usage/configuration/sinks
[docs.sources]: https://docs.vector.dev/usage/configuration/sources
[docs.transforms]: https://docs.vector.dev/usage/configuration/transforms
[docs.use_cases]: https://docs.vector.dev/use-cases
[urls.default_configuration]: https://github.com/timberio/vector/blob/master/config/vector.toml
[urls.docker_alpine]: https://hub.docker.com/_/alpine
[urls.docker_debian]: https://hub.docker.com/_/debian
[urls.rust]: https://www.rust-lang.org/
[urls.vector_releases]: https://github.com/timberio/vector/releases
