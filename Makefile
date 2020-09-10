.PHONY: $(MAKECMDGOALS) all
.DEFAULT_GOAL := help
RUN := $(shell realpath $(shell dirname $(firstword $(MAKEFILE_LIST)))/scripts/docker-compose-run.sh)

# Begin OS detection
ifeq ($(OS),Windows_NT) # is Windows_NT on XP, 2000, 7, Vista, 10...
    export OPERATING_SYSTEM := Windows
	export RUST_TARGET ?= "x86_64-unknown-windows-msvc"
    export DEFAULT_FEATURES = default-msvc
else
    export OPERATING_SYSTEM := $(shell uname)  # same as "uname -s"
	export RUST_TARGET ?= "x86_64-unknown-linux-gnu"
    export DEFAULT_FEATURES = default
endif

# Override this with any scopes for testing/benching.
export SCOPE ?= ""
# Override to false to disable autospawning services on integration tests.
export AUTOSPAWN ?= true
# Override to control if services are turned off after integration tests.
export AUTODESPAWN ?= ${AUTOSPAWN}
# Override to true for a bit more log output in your environment building (more coming!)
export VERBOSE ?= false
# Override to set a different Rust toolchain
export RUST_TOOLCHAIN ?= $(shell cat rust-toolchain)
# Override the container tool. Tries podman first and then tries docker.
export CONTAINER_TOOL ?= auto
ifeq ($(CONTAINER_TOOL),auto)
	override CONTAINER_TOOL = $(shell podman version >/dev/null 2>&1 && echo podman || echo docker)
endif
# If we're using podman create pods else if we're using docker create networks.
ifeq ($(CONTAINER_TOOL),podman)
    export CONTAINER_ENCLOSURE = "pod"
else
    export CONTAINER_ENCLOSURE = "network"
endif

# Override this to automatically enter a container containing the correct, full, official build environment for Vector, ready for development
export ENVIRONMENT ?= false
# The upstream container we publish artifacts to on a successful master build.
export ENVIRONMENT_UPSTREAM ?= docker.pkg.github.com/timberio/vector/environment
# Override to disable building the container, having it pull from the Github packages repo instead
# TODO: Disable this by default. Blocked by `docker pull` from Github Packages requiring authenticated login
export ENVIRONMENT_AUTOBUILD ?= true
# Override this when appropriate to disable a TTY being available in commands with `ENVIRONMENT=true` (Useful for CI, but CI uses Nix!)
export ENVIRONMENT_TTY ?= true
# A list of WASM modules by name
export WASM_MODULES = $(patsubst tests/data/wasm/%/,%,$(wildcard tests/data/wasm/*/))
# The same WASM modules, by output path.
export WASM_MODULE_OUTPUTS = $(patsubst %,/target/wasm32-wasi/%,$(WASM_MODULES))

# Set dummy AWS credentials if not present - used for AWS and ES integration tests
export AWS_ACCESS_KEY_ID ?= "dummy"
export AWS_SECRET_ACCESS_KEY ?= "dummy"

FORMATTING_BEGIN_YELLOW = \033[0;33m
FORMATTING_BEGIN_BLUE = \033[36m
FORMATTING_END = \033[0m

help:
	@printf -- "${FORMATTING_BEGIN_BLUE}                                      __   __  __${FORMATTING_END}\n"
	@printf -- "${FORMATTING_BEGIN_BLUE}                                      \ \ / / / /${FORMATTING_END}\n"
	@printf -- "${FORMATTING_BEGIN_BLUE}                                       \ V / / / ${FORMATTING_END}\n"
	@printf -- "${FORMATTING_BEGIN_BLUE}                                        \_/  \/  ${FORMATTING_END}\n"
	@printf -- "\n"
	@printf -- "                                      V E C T O R\n"
	@printf -- "\n"
	@printf -- "---------------------------------------------------------------------------------------\n"
	@printf -- "Nix user? You can use ${FORMATTING_BEGIN_YELLOW}\`direnv allow .\`${FORMATTING_END} or ${FORMATTING_BEGIN_YELLOW}\`nix-shell --pure\`${FORMATTING_END}\n"
	@printf -- "Want to use ${FORMATTING_BEGIN_YELLOW}\`docker\`${FORMATTING_END} or ${FORMATTING_BEGIN_YELLOW}\`podman\`${FORMATTING_END}? See ${FORMATTING_BEGIN_YELLOW}\`ENVIRONMENT=true\`${FORMATTING_END} commands. (Default ${FORMATTING_BEGIN_YELLOW}\`CONTAINER_TOOL=docker\`${FORMATTING_END})\n"
	@printf -- "\n"
	@awk 'BEGIN {FS = ":.*##"; printf "Usage: make ${FORMATTING_BEGIN_BLUE}<target>${FORMATTING_END}\n"} /^[a-zA-Z0-9_-]+:.*?##/ { printf "  ${FORMATTING_BEGIN_BLUE}%-46s${FORMATTING_END} %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Environment

# These are some predefined macros, please use them!
ifeq ($(ENVIRONMENT), true)
define MAYBE_ENVIRONMENT_EXEC
${ENVIRONMENT_EXEC}
endef
else
define MAYBE_ENVIRONMENT_EXEC

endef
endif

ifeq ($(ENVIRONMENT), true)
define MAYBE_ENVIRONMENT_COPY_ARTIFACTS
${ENVIRONMENT_COPY_ARTIFACTS}
endef
else
define MAYBE_ENVIRONMENT_COPY_ARTIFACTS

endef
endif

# We use a volume here as non-Linux hosts are extremely slow to share disks, and Linux hosts tend to get permissions clobbered.
define ENVIRONMENT_EXEC
	${ENVIRONMENT_PREPARE}
	@echo "Entering environment..."
	@mkdir -p target
	$(CONTAINER_TOOL) run \
			--name vector-environment \
			--rm \
			$(if $(findstring true,$(ENVIRONMENT_TTY)),--tty,) \
			--init \
			--interactive \
			--env INSIDE_ENVIRONMENT=true \
			--network host \
			--mount type=bind,source=${PWD},target=/git/timberio/vector \
			--mount type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock \
			--mount type=volume,source=vector-target,target=/git/timberio/vector/target \
			--mount type=volume,source=vector-cargo-cache,target=/root/.cargo \
			$(ENVIRONMENT_UPSTREAM)
endef

define ENVIRONMENT_COPY_ARTIFACTS
	@echo "Copying artifacts off volumes... (Docker errors below are totally okay)"
	@mkdir -p ./target/release
	@mkdir -p ./target/debug
	@mkdir -p ./target/criterion
	@$(CONTAINER_TOOL) rm -f vector-build-outputs || true
	@$(CONTAINER_TOOL) run \
		-d \
		-v vector-target:/target \
		--name vector-build-outputs \
		busybox true
	@$(CONTAINER_TOOL) cp vector-build-outputs:/target/release/vector ./target/release/ || true
	@$(CONTAINER_TOOL) cp vector-build-outputs:/target/debug/vector ./target/debug/ || true
	@$(CONTAINER_TOOL) cp vector-build-outputs:/target/criterion ./target/criterion || true
	@$(CONTAINER_TOOL) rm -f vector-build-outputs
endef


ifeq ($(ENVIRONMENT_AUTOBUILD), true)
define ENVIRONMENT_PREPARE
	@echo "Building the environment. (ENVIRONMENT_AUTOBUILD=true) This may take a few minutes..."
	$(CONTAINER_TOOL) build \
		$(if $(findstring true,$(VERBOSE)),,--quiet) \
		--tag $(ENVIRONMENT_UPSTREAM) \
		--file scripts/environment/Dockerfile \
		.
endef
else
define ENVIRONMENT_PREPARE
	$(CONTAINER_TOOL) pull $(ENVIRONMENT_UPSTREAM)
endef
endif

check-container-tool: ## Checks what container tool is installed
	@echo -n "Checking if $(CONTAINER_TOOL) is available..." && \
	$(CONTAINER_TOOL) version 1>/dev/null && echo "yes"

environment: export ENVIRONMENT_TTY = true ## Enter a full Vector dev shell in $CONTAINER_TOOL, binding this folder to the container.
environment:
	${ENVIRONMENT_EXEC}

environment-prepare: ## Prepare the Vector dev shell using $CONTAINER_TOOL.
	${ENVIRONMENT_PREPARE}

environment-clean: ## Clean the Vector dev shell using $CONTAINER_TOOL.
	@$(CONTAINER_TOOL) volume rm -f vector-target vector-cargo-cache
	@$(CONTAINER_TOOL) rmi $(ENVIRONMENT_UPSTREAM) || true

environment-push: environment-prepare ## Publish a new version of the container image.
	$(CONTAINER_TOOL) push $(ENVIRONMENT_UPSTREAM)

##@ Building
build: ## Build the project in release mode (Supports `ENVIRONMENT=true`)
	${MAYBE_ENVIRONMENT_EXEC} cargo build --release --no-default-features --features ${DEFAULT_FEATURES}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

build-dev: ## Build the project in development mode (Supports `ENVIRONMENT=true`)
	${MAYBE_ENVIRONMENT_EXEC} cargo build --no-default-features --features ${DEFAULT_FEATURES}

build-all: build-x86_64-unknown-linux-musl build-aarch64-unknown-linux-musl ## Build the project in release mode for all supported platforms

build-x86_64-unknown-linux-gnu: ## Build dynamically linked binary in release mode for the x86_64 architecture
	$(RUN) build-x86_64-unknown-linux-gnu

build-x86_64-unknown-linux-musl: ## Build static binary in release mode for the x86_64 architecture
	$(RUN) build-x86_64-unknown-linux-musl

build-aarch64-unknown-linux-musl: load-qemu-binfmt ## Build static binary in release mode for the aarch64 architecture
	$(RUN) build-aarch64-unknown-linux-musl

##@ Testing (Supports `ENVIRONMENT=true`)

test: ## Run the unit test suite
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features ${DEFAULT_FEATURES} ${SCOPE} -- --nocapture

test-all: test test-behavior test-integration ## Runs all tests, unit, behaviorial, and integration.

test-behavior: ## Runs behaviorial test
	${MAYBE_ENVIRONMENT_EXEC} cargo run -- test tests/behavior/**/*.toml

test-integration: ## Runs all integration tests
test-integration: test-integration-aws test-integration-clickhouse test-integration-docker test-integration-elasticsearch
test-integration: test-integration-gcp test-integration-influxdb test-integration-kafka test-integration-loki
test-integration: test-integration-pulsar test-integration-splunk

start-test-integration: ## Starts all integration test infrastructure
start-test-integration: start-integration-aws start-integration-clickhouse start-integration-elasticsearch
start-test-integration: start-integration-gcp start-integration-influxdb start-integration-kafka start-integration-loki
start-test-integration: start-integration-pulsar start-integration-splunk

stop-test-integration: ## Stops all integration test infrastructure
stop-test-integration: stop-integration-aws stop-integration-clickhouse stop-integration-elasticsearch
stop-test-integration: stop-integration-gcp stop-integration-influxdb stop-integration-kafka stop-integration-loki
stop-test-integration: stop-integration-pulsar stop-integration-splunk

start-integration-aws:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-aws -p 8111:8111 -p 4568:4568 -p 4572:4572 -p 4582:4582 -p 4571:4571 -p 4573:4573 -p 6000:6000
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws --name vector_ec2_metadata \
	 timberiodev/mock-ec2-metadata:latest
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws --name vector_localstack_aws \
	 -e SERVICES=kinesis:4568,s3:4572,cloudwatch:4582,elasticsearch:4571,firehose:4573 \
	 localstack/localstack@sha256:f21f1fc770ee4bfd5012afdc902154c56b7fb18c14cf672de151b65569c8251e
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws --name vector_mockwatchlogs \
	 -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-aws
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws -p 8111:8111 --name vector_ec2_metadata \
	 timberiodev/mock-ec2-metadata:latest
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws -p 4568:4568 -p 4572:4572 \
	 -p 4582:4582 -p 4571:4571 -p 4573:4573 --name vector_localstack_aws \
	 -e SERVICES=kinesis:4568,s3:4572,cloudwatch:4582,elasticsearch:4571,firehose:4573 \
	 localstack/localstack@sha256:f21f1fc770ee4bfd5012afdc902154c56b7fb18c14cf672de151b65569c8251e
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws -p 6000:6000 --name vector_mockwatchlogs \
	 -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
endif

stop-integration-aws:
	$(CONTAINER_TOOL) rm --force vector_ec2_metadata vector_mockwatchlogs vector_localstack_aws 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-aws 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name=vector-test-integration-aws 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-aws 2>/dev/null; true
endif

test-integration-aws: ## Runs AWS integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-aws
	$(MAKE) start-integration-aws
	sleep 5 # Many services are very slow... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features aws-integration-tests --lib ::aws_ -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-aws
endif

start-integration-clickhouse:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-clickhouse -p 8123:8123
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-clickhouse --name vector_clickhouse yandex/clickhouse-server:19
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-clickhouse
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-clickhouse -p 8123:8123 --name vector_clickhouse yandex/clickhouse-server:19
endif

stop-integration-clickhouse:
	$(CONTAINER_TOOL) rm --force vector_clickhouse 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-clickhouse 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-clickhouse 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-clickhouse 2>/dev/null; true
endif

test-integration-clickhouse: ## Runs Clickhouse integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-clickhouse
	$(MAKE) start-integration-clickhouse
	sleep 5 # Many services are very slow... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features clickhouse-integration-tests --lib ::clickhouse:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-clickhouse
endif

test-integration-docker: ## Runs Docker integration tests
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features docker-integration-tests --lib ::docker:: -- --nocapture

start-integration-elasticsearch:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-elasticsearch -p 4571:4571 -p 9200:9200 -p 9300:9300 -p 9201:9200 -p 9301:9300
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-elasticsearch --name vector_localstack_es \
	 -e SERVICES=elasticsearch:4571 localstack/localstack@sha256:f21f1fc770ee4bfd5012afdc902154c56b7fb18c14cf672de151b65569c8251e
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-elasticsearch \
	 --name vector_elasticsearch -e discovery.type=single-node -e ES_JAVA_OPTS="-Xms400m -Xmx400m" elasticsearch:6.6.2
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-elasticsearch \
	 --name vector_elasticsearch-tls -e discovery.type=single-node -e xpack.security.enabled=true \
	 -e xpack.security.http.ssl.enabled=true -e xpack.security.transport.ssl.enabled=true \
	 -e xpack.ssl.certificate=certs/localhost.crt -e xpack.ssl.key=certs/localhost.key \
	 -e ES_JAVA_OPTS="-Xms400m -Xmx400m" \
	 -v $(PWD)/tests/data:/usr/share/elasticsearch/config/certs:ro elasticsearch:6.6.2
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-elasticsearch
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-elasticsearch -p 4571:4571 --name vector_localstack_es \
	 -e SERVICES=elasticsearch:4571 localstack/localstack@sha256:f21f1fc770ee4bfd5012afdc902154c56b7fb18c14cf672de151b65569c8251e
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-elasticsearch -p 9200:9200 -p 9300:9300 \
	 --name vector_elasticsearch -e discovery.type=single-node -e ES_JAVA_OPTS="-Xms400m -Xmx400m" elasticsearch:6.6.2
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-elasticsearch -p 9201:9200 -p 9301:9300 \
	 --name vector_elasticsearch-tls -e discovery.type=single-node -e xpack.security.enabled=true \
	 -e xpack.security.http.ssl.enabled=true -e xpack.security.transport.ssl.enabled=true \
	 -e xpack.ssl.certificate=certs/localhost.crt -e xpack.ssl.key=certs/localhost.key \
	 -e ES_JAVA_OPTS="-Xms400m -Xmx400m" \
	 -v $(PWD)/tests/data:/usr/share/elasticsearch/config/certs:ro elasticsearch:6.6.2
endif

stop-integration-elasticsearch:
	$(CONTAINER_TOOL) rm --force vector_localstack_es vector_elasticsearch vector_elasticsearch-tls 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-elasticsearch 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-elasticsearch 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-elasticsearch 2>/dev/null; true
endif

test-integration-elasticsearch: ## Runs Elasticsearch integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-elasticsearch
	$(MAKE) start-integration-elasticsearch
	sleep 60 # Many services are very slow... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features es-integration-tests --lib ::elasticsearch:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-elasticsearch
endif

start-integration-gcp:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-gcp -p 8681-8682:8681-8682
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-gcp --name vector_cloud-pubsub \
	 -e PUBSUB_PROJECT1=testproject,topic1:subscription1 messagebird/gcloud-pubsub-emulator
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-gcp
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-gcp -p 8681-8682:8681-8682 --name vector_cloud-pubsub \
	 -e PUBSUB_PROJECT1=testproject,topic1:subscription1 messagebird/gcloud-pubsub-emulator
endif

stop-integration-gcp:
	$(CONTAINER_TOOL) rm --force vector_cloud-pubsub 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-gcp 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-gcp 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-gcp 2>/dev/null; true
endif

test-integration-gcp: ## Runs GCP integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-gcp
	$(MAKE) start-integration-gcp
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features gcp-integration-tests --lib ::gcp:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-gcp
endif

start-integration-humio:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-humio -p 8080:8080
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-humio --name vector_humio humio/humio:1.13.1
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-humio
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-humio -p 8080:8080 --name vector_humio humio/humio:1.13.1
endif

stop-integration-humio:
	$(CONTAINER_TOOL) rm --force vector_humio 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-humio 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-humio 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-humio 2>/dev/null; true
endif

test-integration-humio: ## Runs Humio integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-humio
	$(MAKE) start-integration-humio
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features humio-integration-tests --lib ::humio:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-humio
endif

start-integration-influxdb:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-influxdb -p 8086:8086 -p 9999:9999
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb --name vector_influxdb_v1 \
	 -e INFLUXDB_REPORTING_DISABLED=true influxdb:1.7
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb --name vector_influxdb_v2 \
	 -e INFLUXDB_REPORTING_DISABLED=true  quay.io/influxdb/influxdb:2.0.0-beta influxd --reporting-disabled
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-influxdb
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb -p 8086:8086 --name vector_influxdb_v1 \
	 -e INFLUXDB_REPORTING_DISABLED=true influxdb:1.7
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb -p 9999:9999 --name vector_influxdb_v2 \
	 -e INFLUXDB_REPORTING_DISABLED=true  quay.io/influxdb/influxdb:2.0.0-beta influxd --reporting-disabled
endif

stop-integration-influxdb:
	$(CONTAINER_TOOL) rm --force vector_influxdb_v1 vector_influxdb_v2 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-influxdb 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-influxdb 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-influxdb 2>/dev/null; true
endif

test-integration-influxdb: ## Runs InfluxDB integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-influxdb
	$(MAKE) start-integration-influxdb
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features influxdb-integration-tests --lib ::influxdb::integration_tests:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-influxdb
endif

start-integration-kafka:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-kafka -p 2181:2181 -p 9091-9093:9091-9093
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-kafka --name vector_zookeeper wurstmeister/zookeeper
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-kafka -e KAFKA_BROKER_ID=1 \
	 -e KAFKA_ZOOKEEPER_CONNECT=vector_zookeeper:2181 -e KAFKA_LISTENERS=PLAINTEXT://:9091,SSL://:9092,SASL_PLAINTEXT://:9093 \
	 -e KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://localhost:9091,SSL://localhost:9092,SASL_PLAINTEXT://localhost:9093 \
	 -e KAFKA_SSL_KEYSTORE_LOCATION=/certs/localhost.p12 -e KAFKA_SSL_KEYSTORE_PASSWORD=NOPASS \
	 -e KAFKA_SSL_TRUSTSTORE_LOCATION=/certs/localhost.p12 -e KAFKA_SSL_TRUSTSTORE_PASSWORD=NOPASS \
	 -e KAFKA_SSL_KEY_PASSWORD=NOPASS -e KAFKA_SSL_ENDPOINT_IDENTIFICATION_ALGORITHM=none \
	 -e KAFKA_OPTS="-Djava.security.auth.login.config=/etc/kafka/kafka_server_jaas.conf" \
	 -e KAFKA_INTER_BROKER_LISTENER_NAME=SASL_PLAINTEXT -e KAFKA_SASL_ENABLED_MECHANISMS=PLAIN \
	 -e KAFKA_SASL_MECHANISM_INTER_BROKER_PROTOCOL=PLAIN -v $(PWD)/tests/data/localhost.p12:/certs/localhost.p12:ro \
	 -v $(PWD)/tests/data/kafka_server_jaas.conf:/etc/kafka/kafka_server_jaas.conf --name vector_kafka wurstmeister/kafka
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-kafka
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-kafka -p 2181:2181 --name vector_zookeeper wurstmeister/zookeeper
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-kafka -p 9091-9093:9091-9093 -e KAFKA_BROKER_ID=1 \
	 -e KAFKA_ZOOKEEPER_CONNECT=vector_zookeeper:2181 -e KAFKA_LISTENERS=PLAINTEXT://:9091,SSL://:9092,SASL_PLAINTEXT://:9093 \
	 -e KAFKA_ADVERTISED_LISTENERS=PLAINTEXT://localhost:9091,SSL://localhost:9092,SASL_PLAINTEXT://localhost:9093 \
	 -e KAFKA_SSL_KEYSTORE_LOCATION=/certs/localhost.p12 -e KAFKA_SSL_KEYSTORE_PASSWORD=NOPASS \
	 -e KAFKA_SSL_TRUSTSTORE_LOCATION=/certs/localhost.p12 -e KAFKA_SSL_TRUSTSTORE_PASSWORD=NOPASS \
	 -e KAFKA_SSL_KEY_PASSWORD=NOPASS -e KAFKA_SSL_ENDPOINT_IDENTIFICATION_ALGORITHM=none \
	 -e KAFKA_OPTS="-Djava.security.auth.login.config=/etc/kafka/kafka_server_jaas.conf" \
	 -e KAFKA_INTER_BROKER_LISTENER_NAME=SASL_PLAINTEXT -e KAFKA_SASL_ENABLED_MECHANISMS=PLAIN \
	 -e KAFKA_SASL_MECHANISM_INTER_BROKER_PROTOCOL=PLAIN -v $(PWD)/tests/data/localhost.p12:/certs/localhost.p12:ro \
	 -v $(PWD)/tests/data/kafka_server_jaas.conf:/etc/kafka/kafka_server_jaas.conf --name vector_kafka wurstmeister/kafka
endif

stop-integration-kafka:
	$(CONTAINER_TOOL) rm --force vector_kafka vector_zookeeper 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-kafka 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-kafka 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-kafka 2>/dev/null; true
endif

test-integration-kafka: ## Runs Kafka integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-kafka
	$(MAKE) start-integration-kafka
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features "kafka-integration-tests rdkafka-plain" --lib ::kafka:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-kafka
endif

start-integration-loki:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-loki -p 3100:3100
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-loki -v $(PWD)/tests/data:/etc/loki \
	 --name vector_loki grafana/loki:master -config.file=/etc/loki/loki-config.yaml
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-loki
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-loki -p 3100:3100 -v $(PWD)/tests/data:/etc/loki \
	 --name vector_loki grafana/loki:master -config.file=/etc/loki/loki-config.yaml
endif

stop-integration-loki:
	$(CONTAINER_TOOL) rm --force vector_loki 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-loki 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-loki 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-loki 2>/dev/null; true
endif

test-integration-loki: ## Runs Loki integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-loki
	$(MAKE) start-integration-loki
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features loki-integration-tests --lib ::loki:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-loki
endif

start-integration-pulsar:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-pulsar -p 6650:6650
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-pulsar  --name vector_pulsar \
	 apachepulsar/pulsar bin/pulsar standalone
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-pulsar
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-pulsar -p 6650:6650 --name vector_pulsar \
	 apachepulsar/pulsar bin/pulsar standalone
endif

stop-integration-pulsar:
	$(CONTAINER_TOOL) rm --force vector_pulsar 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-pulsar 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-pulsar 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-pulsar 2>/dev/null; true
endif

test-integration-pulsar: ## Runs Pulsar integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-pulsar
	$(MAKE) start-integration-pulsar
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features pulsar-integration-tests --lib ::pulsar:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-pulsar
endif

start-integration-splunk:
# TODO Replace  timberio/splunk-hec-test:minus_compose image with production image once merged
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-splunk -p 8088:8088 -p 8000:8000 -p 8089:8089
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-splunk \
     --name splunk timberio/splunk-hec-test:minus_compose
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-splunk
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-splunk -p 8088:8088 -p 8000:8000 -p 8089:8089 \
     --name splunk timberio/splunk-hec-test:minus_compose
endif

stop-integration-splunk:
	$(CONTAINER_TOOL) rm --force splunk 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-splunk 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-splunk 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-splunk 2>/dev/null; true
endif

test-integration-splunk: ## Runs Splunk integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-splunk
	$(MAKE) start-integration-splunk
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features splunk-integration-tests --lib ::splunk_hec:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-splunk
endif

test-e2e-kubernetes: ## Runs Kubernetes E2E tests (Sorry, no `ENVIRONMENT=true` support)
	@scripts/test-e2e-kubernetes.sh

test-shutdown: ## Runs shutdown tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-kafka
	$(MAKE) start-integration-kafka
	sleep 30 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --features shutdown-tests --test shutdown -- --test-threads 4
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-kafka
endif

test-cli: ## Runs cli tests
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-default-features --test cli -- --test-threads 4

.PHONY: build-wasm-tests
test-wasm-build-modules: $(WASM_MODULE_OUTPUTS) ### Build all WASM test modules

$(WASM_MODULE_OUTPUTS): MODULE = $(notdir $@)
$(WASM_MODULE_OUTPUTS): ### Build a specific WASM module
	@echo "# Building WASM module ${MODULE}, requires Rustc for wasm32-wasi."
	${MAYBE_ENVIRONMENT_EXEC} cargo build \
		--target-dir target/ \
		--manifest-path tests/data/wasm/${MODULE}/Cargo.toml \
		--target wasm32-wasi \
		--release \
		--package ${MODULE}

.PHONY: test-wasm
test-wasm: export TEST_THREADS=1
test-wasm: export TEST_LOG=vector=trace
test-wasm: $(WASM_MODULE_OUTPUTS)  ### Run engine tests
	${MAYBE_ENVIRONMENT_EXEC} cargo test wasm --no-default-features --features "wasm" --lib -- --nocapture

##@ Benching (Supports `ENVIRONMENT=true`)

bench: ## Run benchmarks in /benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "${DEFAULT_FEATURES}"
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: bench-wasm
bench-wasm: $(WASM_MODULE_OUTPUTS)  ### Run WASM benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "${DEFAULT_FEATURES} transforms-wasm transforms-lua" --bench wasm wasm

##@ Checking

check: ## Run prerequisite code checks
	${MAYBE_ENVIRONMENT_EXEC} cargo check --all --no-default-features --features ${DEFAULT_FEATURES}

check-all: check-fmt check-clippy check-style check-markdown check-meta check-version check-examples check-component-features check-scripts ## Check everything

check-component-features: ## Check that all component features are setup properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-component-features.sh

check-clippy: ## Check code with Clippy
	${MAYBE_ENVIRONMENT_EXEC} cargo clippy --workspace --all-targets -- -D warnings

check-fmt: ## Check that all files are formatted properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-fmt.sh

check-style: ## Check that all files are styled properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-style.sh

check-markdown: ## Check that markdown is styled properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-markdown.sh

check-meta: ## Check that all /.meta file are valid
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-meta.sh

check-version: ## Check that Vector's version is correct accounting for recent changes
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-version.rb

check-examples: ## Check that the config/examples files are valid
	${MAYBE_ENVIRONMENT_EXEC} cargo run -- validate --topology --deny-warnings ./config/examples/*.toml

check-scripts: ## Check that scipts do not have common mistakes
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-scripts.sh

check-helm: ## Check that the Helm Chart passes helm lint
	${MAYBE_ENVIRONMENT_EXEC} helm lint distribution/helm/vector

##@ Packaging

package-all: package-archive-all package-deb-all package-rpm-all ## Build all packages

package-x86_64-unknown-linux-musl-all: package-archive-x86_64-unknown-linux-musl package-deb-x86_64 package-rpm-x86_64 # Build all x86_64 MUSL packages


package-x86_64-unknown-linux-musl-all: package-archive-x86_64-unknown-linux-musl # Build all x86_64 MUSL packages

package-x86_64-unknown-linux-gnu-all: package-archive-x86_64-unknown-linux-gnu package-deb-x86_64 package-rpm-x86_64 # Build all x86_64 GNU packages

package-aarch64-unknown-linux-musl-all: package-archive-aarch64-unknown-linux-musl package-deb-aarch64 package-rpm-aarch64  # Build all aarch64 MUSL packages

# archives

package-archive: build ## Build the Vector archive
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/package-archive.sh

package-archive-all: package-archive-x86_64-unknown-linux-musl package-archive-x86_64-unknown-linux-gnu package-archive-aarch64-unknown-linux-musl ## Build all archives

package-archive-x86_64-unknown-linux-musl: build-x86_64-unknown-linux-musl ## Build the x86_64 archive
	$(RUN) package-archive-x86_64-unknown-linux-musl

package-archive-x86_64-unknown-linux-gnu: build-x86_64-unknown-linux-gnu ## Build the x86_64 archive
	$(RUN) package-archive-x86_64-unknown-linux-gnu

package-archive-aarch64-unknown-linux-musl: build-aarch64-unknown-linux-musl ## Build the aarch64 archive
	$(RUN) package-archive-aarch64-unknown-linux-musl

# debs

package-deb: ## Build the deb package
	$(RUN) package-deb

package-deb-all: package-deb-x86_64 ## Build all deb packages

package-deb-x86_64: package-archive-x86_64-unknown-linux-gnu ## Build the x86_64 deb package
	$(RUN) package-deb-x86_64

package-deb-aarch64: package-archive-aarch64-unknown-linux-musl  ## Build the aarch64 deb package
	$(RUN) package-deb-aarch64

# rpms

package-rpm: ## Build the rpm package
	@scripts/package-rpm.sh

package-rpm-all: package-rpm-x86_64 package-rpm-aarch64 ## Build all rpm packages

package-rpm-x86_64: package-archive-x86_64-unknown-linux-gnu ## Build the x86_64 rpm package
	$(RUN) package-rpm-x86_64

package-rpm-aarch64: package-archive-aarch64-unknown-linux-musl ## Build the aarch64 rpm package
	$(RUN) package-rpm-aarch64

##@ Releasing

release: release-prepare generate release-commit ## Release a new Vector version

release-commit: ## Commits release changes
	@scripts/release-commit.rb

release-docker: ## Release to Docker Hub
	@scripts/release-docker.sh

release-github: ## Release to Github
	@scripts/release-github.rb

release-homebrew: ## Release to timberio Homebrew tap
	@scripts/release-homebrew.sh

release-prepare: ## Prepares the release with metadata and highlights
	@scripts/release-prepare.rb

release-push: ## Push new Vector version
	@scripts/release-push.sh

release-rollback: ## Rollback pending release changes
	@scripts/release-rollback.rb

release-s3: ## Release artifacts to S3
	@scripts/release-s3.sh

release-helm: ## Package and release Helm Chart
	@scripts/release-helm.sh

sync-install: ## Sync the install.sh script for access via sh.vector.dev
	@aws s3 cp distribution/install.sh s3://sh.vector.dev --sse --acl public-read

##@ Verifying

verify: verify-rpm verify-deb ## Default target, verify all packages

verify-rpm: verify-rpm-amazonlinux-1 verify-rpm-amazonlinux-2 verify-rpm-centos-7 ## Verify all rpm packages

verify-rpm-amazonlinux-1: package-rpm-x86_64 ## Verify the rpm package on Amazon Linux 1
	$(RUN) verify-rpm-amazonlinux-1

verify-rpm-amazonlinux-2: package-rpm-x86_64 ## Verify the rpm package on Amazon Linux 2
	$(RUN) verify-rpm-amazonlinux-2

verify-rpm-centos-7: package-rpm-x86_64 ## Verify the rpm package on CentOS 7
	$(RUN) verify-rpm-centos-7

verify-deb: ## Verify all deb packages
verify-deb: verify-deb-artifact-on-deb-8 verify-deb-artifact-on-deb-9 verify-deb-artifact-on-deb-10
verify-deb: verify-deb-artifact-on-ubuntu-14-04 verify-deb-artifact-on-ubuntu-16-04 verify-deb-artifact-on-ubuntu-18-04 verify-deb-artifact-on-ubuntu-20-04

verify-deb-artifact-on-deb-8: package-deb-x86_64 ## Verify the deb package on Debian 8
	$(RUN) verify-deb-artifact-on-deb-8

verify-deb-artifact-on-deb-9: package-deb-x86_64 ## Verify the deb package on Debian 9
	$(RUN) verify-deb-artifact-on-deb-9

verify-deb-artifact-on-deb-10: package-deb-x86_64 ## Verify the deb package on Debian 10
	$(RUN) verify-deb-artifact-on-deb-10

verify-deb-artifact-on-ubuntu-14-04: package-deb-x86_64 ## Verify the deb package on Ubuntu 14.04
	$(RUN) verify-deb-artifact-on-ubuntu-14-04

verify-deb-artifact-on-ubuntu-16-04: package-deb-x86_64 ## Verify the deb package on Ubuntu 16.04
	$(RUN) verify-deb-artifact-on-ubuntu-16-04

verify-deb-artifact-on-ubuntu-18-04: package-deb-x86_64 ## Verify the deb package on Ubuntu 18.04
	$(RUN) verify-deb-artifact-on-ubuntu-18-04

verify-deb-artifact-on-ubuntu-20-04: package-deb-x86_64 ## Verify the deb package on Ubuntu 20.04
	$(RUN) verify-deb-artifact-on-ubuntu-20-04

verify-nixos:  ## Verify that Vector can be built on NixOS
	$(RUN) verify-nixos

##@ Utility

build-ci-docker-images: ## Rebuilds all Docker images used in CI
	@scripts/build-ci-docker-images.sh

clean: environment-clean ## Clean everything
	cargo clean

fmt: ## Format code
	${MAYBE_ENVIRONMENT_EXEC} cargo fmt
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-style.sh --fix

init-target-dir: ## Create target directory owned by the current user
	$(RUN) init-target-dir

load-qemu-binfmt: ## Load `binfmt-misc` kernel module which required to use `qemu-user`
	$(RUN) load-qemu-binfmt

signoff: ## Signsoff all previous commits since branch creation
	scripts/signoff.sh

slim-builds: ## Updates the Cargo config to product disk optimized builds (for CI, not for users)
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/slim-builds.sh

target-graph: ## Display dependencies between targets in this Makefile
	@cd $(shell realpath $(shell dirname $(firstword $(MAKEFILE_LIST)))) && docker-compose run --rm target-graph $(TARGET)

version: ## Get the current Vector version
	@scripts/version.sh

git-hooks: ## Add Vector-local git hooks for commit sign-off
	@scripts/install-git-hooks.sh

.PHONY: ensure-has-wasm-toolchain ### Configures a wasm toolchain for test artifact building, if required
ensure-has-wasm-toolchain: target/wasm32-wasi/.obtained
target/wasm32-wasi/.obtained:
	@echo "# You should also install WABT for WASM module development!"
	@echo "# You can use your package manager or check https://github.com/WebAssembly/wabt"
	${MAYBE_ENVIRONMENT_EXEC} rustup target add wasm32-wasi
	@mkdir -p target/wasm32-wasi
	@touch target/wasm32-wasi/.obtained
