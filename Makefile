# .PHONY: $(MAKECMDGOALS) all
.DEFAULT_GOAL := help

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
# Override autoinstalling of tools. (Eg `cargo install`)
export AUTOINSTALL ?= false
# Override to true for a bit more log output in your environment building (more coming!)
export VERBOSE ?= false
# Override to set a different Rust toolchain
export RUST_TOOLCHAIN ?= $(shell cat rust-toolchain)
# Override the container tool. Tries docker first and then tries podman.
export CONTAINER_TOOL ?= auto
ifeq ($(CONTAINER_TOOL),auto)
	override CONTAINER_TOOL = $(shell docker version >/dev/null 2>&1 && echo docker || echo podman)
endif
# If we're using podman create pods else if we're using docker create networks.
ifeq ($(CONTAINER_TOOL),podman)
    export CONTAINER_ENCLOSURE = "pod"
else
    export CONTAINER_ENCLOSURE = "network"
endif
export CURRENT_DIR = $(shell pwd)

# Override this to automatically enter a container containing the correct, full, official build environment for Vector, ready for development
export ENVIRONMENT ?= false
# The upstream container we publish artifacts to on a successful master build.
export ENVIRONMENT_UPSTREAM ?= timberio/ci_image
# Override to disable building the container, having it pull from the Github packages repo instead
# TODO: Disable this by default. Blocked by `docker pull` from Github Packages requiring authenticated login
export ENVIRONMENT_AUTOBUILD ?= true
# Override this when appropriate to disable a TTY being available in commands with `ENVIRONMENT=true`
export ENVIRONMENT_TTY ?= true
# A list of WASM modules by name
export WASM_MODULES = $(patsubst tests/data/wasm/%/,%,$(wildcard tests/data/wasm/*/))
# The same WASM modules, by output path.
export WASM_MODULE_OUTPUTS = $(patsubst %,/target/wasm32-wasi/%,$(WASM_MODULES))

# Set dummy AWS credentials if not present - used for AWS and ES integration tests
export AWS_ACCESS_KEY_ID ?= "dummy"
export AWS_SECRET_ACCESS_KEY ?= "dummy"

# Set if you are on the CI and actually want the things to happen. (Non-CI users should never set this.)
export CI ?= false

FORMATTING_BEGIN_YELLOW = \033[0;33m
FORMATTING_BEGIN_BLUE = \033[36m
FORMATTING_END = \033[0m

# "One weird trick!" https://www.gnu.org/software/make/manual/make.html#Syntax-of-Functions
EMPTY:=
SPACE:= ${EMPTY} ${EMPTY}

help:
	@printf -- "${FORMATTING_BEGIN_BLUE}                                      __   __  __${FORMATTING_END}\n"
	@printf -- "${FORMATTING_BEGIN_BLUE}                                      \ \ / / / /${FORMATTING_END}\n"
	@printf -- "${FORMATTING_BEGIN_BLUE}                                       \ V / / / ${FORMATTING_END}\n"
	@printf -- "${FORMATTING_BEGIN_BLUE}                                        \_/  \/  ${FORMATTING_END}\n"
	@printf -- "\n"
	@printf -- "                                      V E C T O R\n"
	@printf -- "\n"
	@printf -- "---------------------------------------------------------------------------------------\n"
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
			--mount type=bind,source=${CURRENT_DIR},target=/git/timberio/vector \
			--mount type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock \
			--mount type=volume,source=vector-target,target=/git/timberio/vector/target \
			--mount type=volume,source=vector-cargo-cache,target=/root/.cargo \
			$(ENVIRONMENT_UPSTREAM)
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

.PHONY: check-container-tool
check-container-tool: ## Checks what container tool is installed
	@echo -n "Checking if $(CONTAINER_TOOL) is available..." && \
	$(CONTAINER_TOOL) version 1>/dev/null && echo "yes"

.PHONY: environment
environment: export ENVIRONMENT_TTY = true ## Enter a full Vector dev shell in $CONTAINER_TOOL, binding this folder to the container.
environment:
	${ENVIRONMENT_EXEC}

.PHONY: environment-prepare
environment-prepare: ## Prepare the Vector dev shell using $CONTAINER_TOOL.
	${ENVIRONMENT_PREPARE}

.PHONY: environment-clean
environment-clean: ## Clean the Vector dev shell using $CONTAINER_TOOL.
	@$(CONTAINER_TOOL) volume rm -f vector-target vector-cargo-cache
	@$(CONTAINER_TOOL) rmi $(ENVIRONMENT_UPSTREAM) || true

.PHONY: environment-push
environment-push: environment-prepare ## Publish a new version of the container image.
	$(CONTAINER_TOOL) push $(ENVIRONMENT_UPSTREAM)

##@ Building
.PHONY: build
build: ## Build the project in release mode (Supports `ENVIRONMENT=true`)
	${MAYBE_ENVIRONMENT_EXEC} cargo build --release --no-default-features --features ${DEFAULT_FEATURES}
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: build-dev
build-dev: ## Build the project in development mode (Supports `ENVIRONMENT=true`)
	${MAYBE_ENVIRONMENT_EXEC} cargo build --no-default-features --features ${DEFAULT_FEATURES}

.PHONY: build-x86_64-unknown-linux-gnu
build-x86_64-unknown-linux-gnu: target/x86_64-unknown-linux-gnu/release/vector ## Build a release binary for the x86_64-unknown-linux-gnu triple.
	@echo "Output to ${<}"

.PHONY: build-aarch64-unknown-linux-gnu
build-aarch64-unknown-linux-gnu: target/aarch64-unknown-linux-gnu/release/vector ## Build a release binary for the aarch64-unknown-linux-gnu triple.
	@echo "Output to ${<}"

.PHONY: build-x86_64-unknown-linux-musl
build-x86_64-unknown-linux-musl: target/x86_64-unknown-linux-musl/release/vector ## Build a release binary for the aarch64-unknown-linux-gnu triple.
	@echo "Output to ${<}"

.PHONY: build-aarch64-unknown-linux-musl
build-aarch64-unknown-linux-musl: target/aarch64-unknown-linux-musl/release/vector ## Build a release binary for the aarch64-unknown-linux-gnu triple.
	@echo "Output to ${<}"

.PHONY: build-armv7-unknown-linux-gnueabihf
build-armv7-unknown-linux-gnueabihf: target/armv7-unknown-linux-gnueabihf/release/vector ## Build a release binary for the armv7-unknown-linux-gnueabihf triple.
	@echo "Output to ${<}"

.PHONY: build-armv7-unknown-linux-musleabihf
build-armv7-unknown-linux-musleabihf: target/armv7-unknown-linux-musleabihf/release/vector ## Build a release binary for the armv7-unknown-linux-musleabihf triple.
	@echo "Output to ${<}"

.PHONY: build-graphql-schema
build-graphql-schema: ## Generate the `schema.json` for Vector's GraphQL API
	${MAYBE_ENVIRONMENT_EXEC} cargo run --bin graphql-schema --no-default-features --features=default-no-api-client

##@ Cross Compiling
.PHONY: cross-enable
cross-enable: cargo-install-cross

.PHONY: CARGO_HANDLES_FRESHNESS
CARGO_HANDLES_FRESHNESS:
	${EMPTY}

# This is basically a shorthand for folks.
# `cross-anything-triple` will call `cross anything --target triple` with the right features.
.PHONY: cross-%
cross-%: export PAIR =$(subst -, ,$($(strip @):cross-%=%))
cross-%: export COMMAND ?=$(word 1,${PAIR})
cross-%: export TRIPLE ?=$(subst ${SPACE},-,$(wordlist 2,99,${PAIR}))
cross-%: export PROFILE ?= release
cross-%: export RUSTFLAGS += -C link-arg=-s
cross-%: cargo-install-cross
	$(MAKE) -k cross-image-${TRIPLE}
	cross ${COMMAND} \
		$(if $(findstring release,$(PROFILE)),--release,) \
		--target ${TRIPLE} \
		--no-default-features \
		--features target-${TRIPLE}

target/%/vector: export PAIR =$(subst /, ,$(@:target/%/vector=%))
target/%/vector: export TRIPLE ?=$(word 1,${PAIR})
target/%/vector: export PROFILE ?=$(word 2,${PAIR})
target/%/vector: export RUSTFLAGS += -C link-arg=-s
target/%/vector: cargo-install-cross CARGO_HANDLES_FRESHNESS
	$(MAKE) -k cross-image-${TRIPLE}
	cross build \
		$(if $(findstring release,$(PROFILE)),--release,) \
		--target ${TRIPLE} \
		--no-default-features \
		--features target-${TRIPLE}

target/%/vector.tar.gz: export PAIR =$(subst /, ,$(@:target/%/vector.tar.gz=%))
target/%/vector.tar.gz: export TRIPLE ?=$(word 1,${PAIR})
target/%/vector.tar.gz: export PROFILE ?=$(word 2,${PAIR})
target/%/vector.tar.gz: target/%/vector CARGO_HANDLES_FRESHNESS
	rm -rf target/scratch/vector-${TRIPLE} || true
	mkdir -p target/scratch/vector-${TRIPLE}/bin target/scratch/vector-${TRIPLE}/etc
	cp --recursive --force --verbose \
		target/${TRIPLE}/${PROFILE}/vector \
		target/scratch/vector-${TRIPLE}/bin/vector
	cp --recursive --force --verbose \
		README.md \
		LICENSE \
		config \
		target/scratch/vector-${TRIPLE}/
	cp --recursive --force --verbose \
		distribution/systemd \
		target/scratch/vector-${TRIPLE}/etc/
	tar --create \
		--gzip \
		--verbose \
		--file target/${TRIPLE}/${PROFILE}/vector.tar.gz \
		--directory target/scratch/ \
		./vector-${TRIPLE}
	rm -rf target/scratch/

.PHONY: cross-image-%
cross-image-%: export TRIPLE =$($(strip @):cross-image-%=%)
cross-image-%:
	$(CONTAINER_TOOL) build \
		--tag vector-cross-env:${TRIPLE} \
		--file scripts/cross/${TRIPLE}.dockerfile \
		scripts/cross

##@ Testing (Supports `ENVIRONMENT=true`)

.PHONY: test
test: ## Run the unit test suite
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --workspace --features ${DEFAULT_FEATURES} ${SCOPE} --all-targets -- --nocapture

.PHONY: test-components
test-components: ## Test with all components enabled
# TODO(jesse) add `wasm-benches` when https://github.com/timberio/vector/issues/5106 is fixed
# test-components: $(WASM_MODULE_OUTPUTS)
test-components: export DEFAULT_FEATURES:="${DEFAULT_FEATURES} benches"
test-components: test

.PHONY: test-all
test-all: test test-behavior test-integration ## Runs all tests, unit, behaviorial, and integration.

.PHONY: test-x86_64-unknown-linux-gnu
test-x86_64-unknown-linux-gnu: cross-test-x86_64-unknown-linux-gnu ## Runs unit tests on the x86_64-unknown-linux-gnu triple
	${EMPTY}

.PHONY: test-aarch64-unknown-linux-gnu
test-aarch64-unknown-linux-gnu: cross-test-aarch64-unknown-linux-gnu ## Runs unit tests on the aarch64-unknown-linux-gnu triple
	${EMPTY}

.PHONY: test-behavior
test-behavior: ## Runs behaviorial test
	${MAYBE_ENVIRONMENT_EXEC} cargo run -- test tests/behavior/**/*

.PHONY: test-integration
test-integration: ## Runs all integration tests
test-integration: test-integration-aws test-integration-clickhouse test-integration-docker-logs test-integration-elasticsearch
test-integration: test-integration-gcp test-integration-humio test-integration-influxdb test-integration-kafka
test-integration: test-integration-loki test-integration-mongodb_metrics test-integration-nats
test-integration: test-integration-nginx test-integration-prometheus test-integration-pulsar test-integration-splunk

.PHONY: start-test-integration
start-test-integration: ## Starts all integration test infrastructure
start-test-integration: start-integration-aws start-integration-clickhouse start-integration-elasticsearch
start-test-integration: start-integration-gcp start-integration-humio start-integration-influxdb start-integration-kafka
start-test-integration: start-integration-loki start-integration-mongodb_metrics start-integration-nats
start-test-integration: start-integration-nginx start-integration-prometheus start-integration-pulsar start-integration-splunk

.PHONY: stop-test-integration
stop-test-integration: ## Stops all integration test infrastructure
stop-test-integration: stop-integration-aws stop-integration-clickhouse stop-integration-elasticsearch
stop-test-integration: stop-integration-gcp stop-integration-humio stop-integration-influxdb stop-integration-kafka
stop-test-integration: stop-integration-loki stop-integration-mongodb_metrics stop-integration-nats
stop-test-integration: stop-integration-nginx stop-integration-prometheus stop-integration-pulsar stop-integration-splunk

.PHONY: start-integration-aws
start-integration-aws:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-aws -p 4566:4566 -p 4571:4571 -p 6000:6000 -p 9088:80
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws --name vector_ec2_metadata \
	 timberiodev/mock-ec2-metadata:latest
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws --name vector_localstack_aws \
	 -e SERVICES=kinesis,s3,cloudwatch,elasticsearch,es,firehose,sqs \
	 localstack/localstack-full:0.11.6
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws --name vector_mockwatchlogs \
	 -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws -v /var/run:/var/run --name vector_local_ecs \
	 -e RUST_LOG=trace amazon/amazon-ecs-local-container-endpoints:latest
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-aws
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws -p 8111:8111 --name vector_ec2_metadata \
	 timberiodev/mock-ec2-metadata:latest
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws --name vector_localstack_aws \
	 -p 4566:4566 -p 4571:4571 \
	 -e SERVICES=kinesis,s3,cloudwatch,elasticsearch,es,firehose,sqs \
	 localstack/localstack-full:0.11.6
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws -p 6000:6000 --name vector_mockwatchlogs \
	 -e RUST_LOG=trace luciofranco/mockwatchlogs:latest
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-aws -v /var/run:/var/run -p 9088:80 --name vector_local_ecs \
	 -e RUST_LOG=trace amazon/amazon-ecs-local-container-endpoints:latest
endif

.PHONY: stop-integration-aws
stop-integration-aws:
	$(CONTAINER_TOOL) rm --force vector_ec2_metadata vector_mockwatchlogs vector_localstack_aws vector_local_ecs 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-aws 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name=vector-test-integration-aws 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-aws 2>/dev/null; true
endif

.PHONY: test-integration-aws
test-integration-aws: ## Runs AWS integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-aws
	$(MAKE) start-integration-aws
	sleep 10 # Many services are very slow... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features aws-integration-tests --lib ::aws_ -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-aws
endif

.PHONY: start-integration-clickhouse
start-integration-clickhouse:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-clickhouse -p 8123:8123
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-clickhouse --name vector_clickhouse yandex/clickhouse-server:19
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-clickhouse
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-clickhouse -p 8123:8123 --name vector_clickhouse yandex/clickhouse-server:19
endif

.PHONY: stop-integration-clickhouse
stop-integration-clickhouse:
	$(CONTAINER_TOOL) rm --force vector_clickhouse 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-clickhouse 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-clickhouse 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-clickhouse 2>/dev/null; true
endif

.PHONY: test-integration-clickhouse
test-integration-clickhouse: ## Runs Clickhouse integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-clickhouse
	$(MAKE) start-integration-clickhouse
	sleep 5 # Many services are very slow... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features clickhouse-integration-tests --lib ::clickhouse:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-clickhouse
endif

.PHONY: test-integration-docker-logs
test-integration-docker-logs: ## Runs Docker Logs integration tests
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features docker-logs-integration-tests --lib ::docker_logs:: -- --nocapture

.PHONY: start-integration-elasticsearch
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

.PHONY: stop-integration-elasticsearch
stop-integration-elasticsearch:
	$(CONTAINER_TOOL) rm --force vector_localstack_es vector_elasticsearch vector_elasticsearch-tls 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-elasticsearch 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-elasticsearch 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-elasticsearch 2>/dev/null; true
endif

.PHONY: test-integration-elasticsearch
test-integration-elasticsearch: ## Runs Elasticsearch integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-elasticsearch
	$(MAKE) start-integration-elasticsearch
	sleep 60 # Many services are very slow... Give them a sec...
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features es-integration-tests --lib ::elasticsearch:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-elasticsearch
endif

.PHONY: start-integration-gcp
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

.PHONY: stop-integration-gcp
stop-integration-gcp:
	$(CONTAINER_TOOL) rm --force vector_cloud-pubsub 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-gcp 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-gcp 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-gcp 2>/dev/null; true
endif

.PHONY: test-integration-gcp
test-integration-gcp: ## Runs GCP integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-gcp
	$(MAKE) start-integration-gcp
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features "gcp-integration-tests gcp-pubsub-integration-tests gcp-cloud-storage-integration-tests" \
	 --lib ::gcp:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-gcp
endif

.PHONY: start-integration-humio
start-integration-humio:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-humio -p 8080:8080
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-humio --name vector_humio humio/humio:1.13.1
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-humio
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-humio -p 8080:8080 --name vector_humio humio/humio:1.13.1
endif

.PHONY: stop-integration-humio
stop-integration-humio:
	$(CONTAINER_TOOL) rm --force vector_humio 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-humio 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-humio 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-humio 2>/dev/null; true
endif

.PHONY: test-integration-humio
test-integration-humio: ## Runs Humio integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-humio
	$(MAKE) start-integration-humio
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features humio-integration-tests --lib "::humio::.*::integration_tests::" -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-humio
endif

.PHONY: start-integration-influxdb
start-integration-influxdb:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-influxdb -p 8086:8086 -p 8087:8087 -p 9999:9999
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb --name vector_influxdb_v1 \
	 -e INFLUXDB_REPORTING_DISABLED=true influxdb:1.8
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb --name vector_influxdb_v1_tls \
	 -e INFLUXDB_REPORTING_DISABLED=true -e INFLUXDB_HTTP_HTTPS_ENABLED=true -e INFLUXDB_HTTP_BIND_ADDRESS=:8087 -e INFLUXDB_BIND_ADDRESS=:8089 \
	 -e INFLUXDB_HTTP_HTTPS_CERTIFICATE=/etc/ssl/localhost.crt -e INFLUXDB_HTTP_HTTPS_PRIVATE_KEY=/etc/ssl/localhost.key \
	 -v $(PWD)/tests/data:/etc/ssl:ro influxdb:1.8
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb --name vector_influxdb_v2 \
	 -e INFLUXDB_REPORTING_DISABLED=true  quay.io/influxdb/influxdb:2.0.0-rc influxd --reporting-disabled --http-bind-address=:9999
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-influxdb
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb -p 8086:8086 --name vector_influxdb_v1 \
	 -e INFLUXDB_REPORTING_DISABLED=true influxdb:1.8
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb -p 8087:8087 --name vector_influxdb_v1_tls \
	 -e INFLUXDB_REPORTING_DISABLED=true -e INFLUXDB_HTTP_HTTPS_ENABLED=true -e INFLUXDB_HTTP_BIND_ADDRESS=:8087 \
	 -e INFLUXDB_HTTP_HTTPS_CERTIFICATE=/etc/ssl/localhost.crt -e INFLUXDB_HTTP_HTTPS_PRIVATE_KEY=/etc/ssl/localhost.key \
	 -v $(PWD)/tests/data:/etc/ssl:ro influxdb:1.8
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-influxdb -p 9999:9999 --name vector_influxdb_v2 \
	 -e INFLUXDB_REPORTING_DISABLED=true  quay.io/influxdb/influxdb:2.0.0-rc influxd --reporting-disabled --http-bind-address=:9999
endif

.PHONY: stop-integration-influxdb
stop-integration-influxdb:
	$(CONTAINER_TOOL) rm --force vector_influxdb_v1 vector_influxdb_v1_tls vector_influxdb_v2 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-influxdb 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-influxdb 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-influxdb 2>/dev/null; true
endif

.PHONY: test-integration-influxdb
test-integration-influxdb: ## Runs InfluxDB integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-influxdb
	$(MAKE) start-integration-influxdb
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features influxdb-integration-tests --lib integration_tests:: --  ::influxdb --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-influxdb
endif

.PHONY: start-integration-kafka
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

.PHONY: stop-integration-kafka
stop-integration-kafka:
	$(CONTAINER_TOOL) rm --force vector_kafka vector_zookeeper 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-kafka 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-kafka 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-kafka 2>/dev/null; true
endif

.PHONY: test-integration-kafka
test-integration-kafka: ## Runs Kafka integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-kafka
	$(MAKE) start-integration-kafka
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features "kafka-integration-tests rdkafka-plain" --lib ::kafka:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-kafka
endif

.PHONY: start-integration-loki
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

.PHONY: stop-integration-loki
stop-integration-loki:
	$(CONTAINER_TOOL) rm --force vector_loki 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-loki 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-loki 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-loki 2>/dev/null; true
endif

.PHONY: test-integration-loki
test-integration-loki: ## Runs Loki integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-loki
	$(MAKE) start-integration-loki
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features loki-integration-tests --lib ::loki:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-loki
endif

# https://docs.mongodb.com/manual/tutorial/deploy-shard-cluster/
.PHONY: start-integration-mongodb_metrics
start-integration-mongodb_metrics:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-mongodb_metrics -p 27017:27017 -p 27018:27018 -p 27019:27019
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-mongodb_metrics --name vector_mongodb_metrics1 mongo:4.2.10 mongod --configsvr --replSet vector
	sleep 1
	$(CONTAINER_TOOL) exec vector_mongodb_metrics1 mongo --port 27019 --eval 'rs.initiate({_id:"vector",configsvr:true,members:[{_id:0,host:"127.0.0.1:27019"}]})'
	$(CONTAINER_TOOL) exec -d vector_mongodb_metrics1 mongos --port 27018 --configdb vector/127.0.0.1:27019
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-mongodb_metrics --name vector_mongodb_metrics2 mongo:4.2.10 mongod
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-mongodb_metrics
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-mongodb_metrics -p 27018:27018 -p 27019:27019 --name vector_mongodb_metrics1 mongo:4.2.10 mongod --configsvr --replSet vector
	sleep 1
	$(CONTAINER_TOOL) exec vector_mongodb_metrics1 mongo --port 27019 --eval 'rs.initiate({_id:"vector",configsvr:true,members:[{_id:0,host:"127.0.0.1:27019"}]})'
	$(CONTAINER_TOOL) exec -d vector_mongodb_metrics1 mongos --port 27018 --configdb vector/127.0.0.1:27019
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-mongodb_metrics -p 27017:27017 --name vector_mongodb_metrics2 mongo:4.2.10 mongod
endif

.PHONY: stop-integration-mongodb_metrics
stop-integration-mongodb_metrics:
	$(CONTAINER_TOOL) rm --force vector_mongodb_metrics1 2>/dev/null; true
	$(CONTAINER_TOOL) rm --force vector_mongodb_metrics2 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-mongodb_metrics 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-mongodb_metrics 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-mongodb_metrics 2>/dev/null; true
endif

.PHONY: test-integration-mongodb_metrics
test-integration-mongodb_metrics: ## Runs MongoDB Metrics integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-mongodb_metrics
	$(MAKE) start-integration-mongodb_metrics
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features mongodb_metrics-integration-tests --lib ::mongodb_metrics:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-mongodb_metrics
endif

.PHONY: start-integration-nats
start-integration-nats:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-nats -p 4222:4222
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-nats  --name vector_nats \
	 nats
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-nats
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-nats -p 4222:4222 --name vector_nats \
	 nats
endif

.PHONY: stop-integration-nats
stop-integration-nats:
	$(CONTAINER_TOOL) rm --force vector_nats 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-nats 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-nats 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-nats 2>/dev/null; true
endif

.PHONY: test-integration-nats
test-integration-nats: ## Runs NATS integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-nats
	$(MAKE) start-integration-nats
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features nats-integration-tests --lib ::nats:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-nats
endif

.PHONY: start-integration-nginx
start-integration-nginx:
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create --replace --name vector-test-integration-nginx -p 8010:8000
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-nginx --name vector_nginx \
	-v $(PWD)tests/data/nginx/:/etc/nginx:ro nginx:1.19.4
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) create vector-test-integration-nginx
	$(CONTAINER_TOOL) run -d --$(CONTAINER_ENCLOSURE)=vector-test-integration-nginx -p 8010:8000 --name vector_nginx \
	-v $(PWD)/tests/data/nginx/:/etc/nginx:ro nginx:1.19.4
endif

.PHONY: stop-integration-nginx
stop-integration-nginx:
	$(CONTAINER_TOOL) rm --force vector_nginx 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-nginx 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-nginx 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-nginx 2>/dev/null; true
endif

.PHONY: test-integration-nginx
test-integration-nginx: ## Runs nginx integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-nginx
	$(MAKE) start-integration-nginx
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features nginx-integration-tests --lib ::nginx:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-nginx
endif

.PHONY: start-integration-prometheus stop-integration-prometheus test-integration-prometheus
start-integration-prometheus:
	$(CONTAINER_TOOL) run -d --name vector_prometheus --net=host \
	 --volume $(PWD)/tests/data:/etc/vector:ro \
	 prom/prometheus --config.file=/etc/vector/prometheus.yaml

stop-integration-prometheus:
	$(CONTAINER_TOOL) rm --force vector_prometheus 2>/dev/null; true

.PHONY: test-integration-prometheus
test-integration-prometheus: ## Runs Prometheus integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-influxdb
	-$(MAKE) -k stop-integration-prometheus
	$(MAKE) start-integration-influxdb
	$(MAKE) start-integration-prometheus
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features prometheus-integration-tests --lib ::prometheus:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-influxdb
	$(MAKE) -k stop-integration-prometheus
endif

.PHONY: start-integration-pulsar
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

.PHONY: stop-integration-pulsar
stop-integration-pulsar:
	$(CONTAINER_TOOL) rm --force vector_pulsar 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-pulsar 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-pulsar 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-pulsar 2>/dev/null; true
endif

.PHONY: test-integration-pulsar
test-integration-pulsar: ## Runs Pulsar integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-pulsar
	$(MAKE) start-integration-pulsar
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features pulsar-integration-tests --lib ::pulsar:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-pulsar
endif

.PHONY: start-integration-splunk
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

.PHONY: stop-integration-splunk
stop-integration-splunk:
	$(CONTAINER_TOOL) rm --force splunk 2>/dev/null; true
ifeq ($(CONTAINER_TOOL),podman)
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) stop --name=vector-test-integration-splunk 2>/dev/null; true
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm --force --name vector-test-integration-splunk 2>/dev/null; true
else
	$(CONTAINER_TOOL) $(CONTAINER_ENCLOSURE) rm vector-test-integration-splunk 2>/dev/null; true
endif

.PHONY: test-integration-splunk
test-integration-splunk: ## Runs Splunk integration tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-splunk
	$(MAKE) start-integration-splunk
	sleep 10 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features splunk-integration-tests --lib ::splunk_hec:: -- --nocapture
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-splunk
endif

.PHONY: test-e2e-kubernetes
test-e2e-kubernetes: ## Runs Kubernetes E2E tests (Sorry, no `ENVIRONMENT=true` support)
	@scripts/test-e2e-kubernetes.sh

.PHONY: test-shutdown
test-shutdown: ## Runs shutdown tests
ifeq ($(AUTOSPAWN), true)
	-$(MAKE) -k stop-integration-kafka
	$(MAKE) start-integration-kafka
	sleep 30 # Many services are very slow... Give them a sec..
endif
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --features shutdown-tests --test shutdown -- --test-threads 4
ifeq ($(AUTODESPAWN), true)
	$(MAKE) -k stop-integration-kafka
endif

.PHONY: test-cli
test-cli: ## Runs cli tests
	${MAYBE_ENVIRONMENT_EXEC} cargo test --no-fail-fast --no-default-features --test cli -- --test-threads 4

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
	${MAYBE_ENVIRONMENT_EXEC} cargo test wasm --no-fail-fast --no-default-features --features "transforms-field_filter transforms-wasm transforms-lua transforms-add_fields" --lib --all-targets -- --nocapture

##@ Benching (Supports `ENVIRONMENT=true`)

.PHONY: bench
bench: ## Run benchmarks in /benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "benches"
	${MAYBE_ENVIRONMENT_COPY_ARTIFACTS}

.PHONY: bench-all
bench-all: ### Run default and WASM benches
bench-all: $(WASM_MODULE_OUTPUTS)
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "benches wasm-benches"

.PHONY: bench-wasm
bench-wasm: $(WASM_MODULE_OUTPUTS)  ### Run WASM benches
	${MAYBE_ENVIRONMENT_EXEC} cargo bench --no-default-features --features "wasm-benches" --bench wasm wasm

##@ Checking

.PHONY: check
check: ## Run prerequisite code checks
	${MAYBE_ENVIRONMENT_EXEC} cargo check --all --no-default-features --features ${DEFAULT_FEATURES}

.PHONY: check-all
check-all: ## Check everything
check-all: check-fmt check-clippy check-style check-markdown check-docs
check-all: check-version check-examples check-component-features
check-all: check-scripts
check-all: check-helm-lint check-helm-dependencies check-kubernetes-yaml

.PHONY: check-component-features
check-component-features: ## Check that all component features are setup properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-component-features.sh

.PHONY: check-clippy
check-clippy: ## Check code with Clippy
	${MAYBE_ENVIRONMENT_EXEC} cargo clippy --workspace --all-targets --features all-integration-tests -- -D warnings

.PHONY: check-docs
check-docs: ## Check that all /docs file are valid
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-docs.sh

.PHONY: check-fmt
check-fmt: ## Check that all files are formatted properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-fmt.sh

.PHONY: check-style
check-style: ## Check that all files are styled properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-style.sh

.PHONY: check-markdown
check-markdown: ## Check that markdown is styled properly
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-markdown.sh

.PHONY: check-version
check-version: ## Check that Vector's version is correct accounting for recent changes
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-version.rb

.PHONY: check-examples
check-examples: ## Check that the config/examples files are valid
	${MAYBE_ENVIRONMENT_EXEC} cargo run -- validate --topology --deny-warnings ./config/examples/*.toml

.PHONY: check-scripts
check-scripts: ## Check that scipts do not have common mistakes
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-scripts.sh

.PHONY: check-helm-lint
check-helm-lint: ## Check that Helm charts pass helm lint
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-helm-lint.sh

.PHONY: check-helm-dependencies
check-helm-dependencies: ## Check that Helm charts have up-to-date dependencies
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/helm-dependencies.sh validate

.PHONY: check-kubernetes-yaml
check-kubernetes-yaml: ## Check that the generated Kubernetes YAML config is up to date
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/kubernetes-yaml.sh check

check-events: ## Check that events satisfy patterns set in https://github.com/timberio/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-events.sh

##@ Packaging

# archives
target/artifacts/vector-%.tar.gz: export TRIPLE :=$(@:target/artifacts/vector-%.tar.gz=%)
target/artifacts/vector-%.tar.gz: override PROFILE =release
target/artifacts/vector-%.tar.gz: target/%/release/vector.tar.gz
	@echo "Built to ${<}, relocating to ${@}"
	@mkdir -p target/artifacts/
	@cp -v \
		${<} \
		${@}

.PHONY: package
package: build ## Build the Vector archive
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/package-archive.sh

.PHONY: package-x86_64-unknown-linux-gnu-all
package-x86_64-unknown-linux-gnu-all: package-x86_64-unknown-linux-gnu package-deb-x86_64-unknown-linux-gnu package-rpm-x86_64-unknown-linux-gnu # Build all x86_64 GNU packages

.PHONY: package-x86_64-unknown-linux-musl-all
package-x86_64-unknown-linux-musl-all: package-x86_64-unknown-linux-musl package-rpm-x86_64-unknown-linux-musl package-deb-x86_64-unknown-linux-musl # Build all x86_64 MUSL packages

.PHONY: package-aarch64-unknown-linux-musl-all
package-aarch64-unknown-linux-musl-all: package-aarch64-unknown-linux-musl package-deb-aarch64 package-rpm-aarch64  # Build all aarch64 MUSL packages

.PHONY: package-armv7-unknown-linux-gnueabihf-all
package-armv7-unknown-linux-gnueabihf-all: package-armv7-unknown-linux-gnueabihf package-deb-armv7-gnu package-rpm-armv7-gnu  # Build all armv7-unknown-linux-gnueabihf MUSL packages

.PHONY: package-x86_64-unknown-linux-gnu
package-x86_64-unknown-linux-gnu: target/artifacts/vector-x86_64-unknown-linux-gnu.tar.gz ## Build an archive suitable for the `x86_64-unknown-linux-gnu` triple.
	@echo "Output to ${<}."

.PHONY: package-x86_64-unknown-linux-musl
package-x86_64-unknown-linux-musl: target/artifacts/vector-x86_64-unknown-linux-musl.tar.gz ## Build an archive suitable for the `x86_64-unknown-linux-musl` triple.
	@echo "Output to ${<}."

.PHONY: package-aarch64-unknown-linux-musl
package-aarch64-unknown-linux-musl: target/artifacts/vector-aarch64-unknown-linux-musl.tar.gz ## Build an archive suitable for the `aarch64-unknown-linux-musl` triple.
	@echo "Output to ${<}."

.PHONY: package-aarch64-unknown-linux-gnu
package-aarch64-unknown-linux-gnu: target/artifacts/vector-aarch64-unknown-linux-gnu.tar.gz ## Build an archive suitable for the `aarch64-unknown-linux-gnu` triple.
	@echo "Output to ${<}."

.PHONY: package-armv7-unknown-linux-gnueabihf
package-armv7-unknown-linux-gnueabihf: target/artifacts/vector-armv7-unknown-linux-gnueabihf.tar.gz ## Build an archive suitable for the `armv7-unknown-linux-gnueabihf` triple.
	@echo "Output to ${<}."

.PHONY: package-armv7-unknown-linux-musleabihf
package-armv7-unknown-linux-musleabihf: target/artifacts/vector-armv7-unknown-linux-musleabihf.tar.gz ## Build an archive suitable for the `armv7-unknown-linux-musleabihf triple.
	@echo "Output to ${<}."

# debs

.PHONY: package-deb-x86_64-unknown-linux-gnu
package-deb-x86_64-unknown-linux-gnu: package-x86_64-unknown-linux-gnu ## Build the x86_64 GNU deb package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=x86_64-unknown-linux-gnu timberio/ci_image ./scripts/package-deb.sh

.PHONY: package-deb-x86_64-unknown-linux-musl
package-deb-x86_64-unknown-linux-musl: package-x86_64-unknown-linux-musl ## Build the x86_64 GNU deb package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=x86_64-unknown-linux-musl timberio/ci_image ./scripts/package-deb.sh

.PHONY: package-deb-aarch64
package-deb-aarch64: package-aarch64-unknown-linux-musl  ## Build the aarch64 deb package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=aarch64-unknown-linux-musl timberio/ci_image ./scripts/package-deb.sh

.PHONY: package-deb-armv7-gnu
package-deb-armv7-gnu: package-armv7-unknown-linux-gnueabihf ## Build the armv7-unknown-linux-gnueabihf deb package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=armv7-unknown-linux-gnueabihf timberio/ci_image ./scripts/package-deb.sh

# rpms

.PHONY: package-rpm-x86_64-unknown-linux-gnu
package-rpm-x86_64-unknown-linux-gnu: package-x86_64-unknown-linux-gnu ## Build the x86_64 rpm package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=x86_64-unknown-linux-gnu timberio/ci_image ./scripts/package-rpm.sh

.PHONY: package-rpm-x86_64-unknown-linux-musl
package-rpm-x86_64-unknown-linux-musl: package-x86_64-unknown-linux-musl ## Build the x86_64 musl rpm package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=x86_64-unknown-linux-musl timberio/ci_image ./scripts/package-rpm.sh

.PHONY: package-rpm-aarch64
package-rpm-aarch64: package-aarch64-unknown-linux-musl ## Build the aarch64 rpm package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=aarch64-unknown-linux-musl timberio/ci_image ./scripts/package-rpm.sh

.PHONY: package-rpm-armv7-gnu
package-rpm-armv7-gnu: package-armv7-unknown-linux-gnueabihf ## Build the armv7-unknown-linux-gnueabihf rpm package
	$(CONTAINER_TOOL) run -v  $(PWD):/git/timberio/vector/ -e TARGET=armv7-unknown-linux-gnueabihf timberio/ci_image ./scripts/package-rpm.sh

##@ Releasing

.PHONY: release
release: release-prepare generate release-commit ## Release a new Vector version

.PHONY: release-commit
release-commit: ## Commits release changes
	@scripts/release-commit.rb

.PHONY: release-docker
release-docker: ## Release to Docker Hub
	@scripts/release-docker.sh

.PHONY: release-github
release-github: ## Release to Github
	@scripts/release-github.sh

.PHONY: release-homebrew
release-homebrew: ## Release to timberio Homebrew tap
	@scripts/release-homebrew.sh

.PHONY: release-prepare
release-prepare: ## Prepares the release with metadata and highlights
	@scripts/release-prepare.rb

.PHONY: release-push
release-push: ## Push new Vector version
	@scripts/release-push.sh

.PHONY: release-rollback
release-rollback: ## Rollback pending release changes
	@scripts/release-rollback.rb

.PHONY: release-s3
release-s3: ## Release artifacts to S3
	@scripts/release-s3.sh

.PHONY: release-helm
release-helm: ## Package and release Helm Chart
	@scripts/release-helm.sh

.PHONY: sync-install
sync-install: ## Sync the install.sh script for access via sh.vector.dev
	@aws s3 cp distribution/install.sh s3://sh.vector.dev --sse --acl public-read

##@ Utility

.PHONY: build-ci-docker-images
build-ci-docker-images: ## Rebuilds all Docker images used in CI
	@scripts/build-ci-docker-images.sh

.PHONY: clean
clean: environment-clean ## Clean everything
	cargo clean

.PHONY: fmt
fmt: ## Format code
	${MAYBE_ENVIRONMENT_EXEC} cargo fmt
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/check-style.sh --fix

.PHONY: signoff
signoff: ## Signsoff all previous commits since branch creation
	scripts/signoff.sh

.PHONY: slim-builds
slim-builds: ## Updates the Cargo config to product disk optimized builds (for CI, not for users)
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/slim-builds.sh

ifeq (${CI}, true)
.PHONY: ci-sweep
ci-sweep: ## Sweep up the CI to try to get more disk space.
	@echo "Preparing the CI for build by sweeping up disk space a bit..."
	df -h
	sudo apt-get --purge autoremove --yes
	sudo apt-get clean
	sudo rm -rf "/opt/*" "/usr/local/*"
	sudo rm -rf "/usr/local/share/boost" && sudo rm -rf "${AGENT_TOOLSDIRECTORY}"
	docker system prune --force
	df -h
endif

.PHONY: version
version: ## Get the current Vector version
	@scripts/version.sh

.PHONY: git-hooks
git-hooks: ## Add Vector-local git hooks for commit sign-off
	@scripts/install-git-hooks.sh

.PHONY: update-helm-dependencies
update-helm-dependencies: ## Recursively update the dependencies of the Helm charts in the proper order
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/helm-dependencies.sh update

.PHONY: update-kubernetes-yaml
update-kubernetes-yaml: ## Regenerate the Kubernetes YAML config
	${MAYBE_ENVIRONMENT_EXEC} ./scripts/kubernetes-yaml.sh update

.PHONY: cargo-install-%
cargo-install-%: override TOOL = $(@:cargo-install-%=%)
cargo-install-%:
	$(if $(findstring true,$(AUTOINSTALL)),cargo install ${TOOL} --quiet,)

.PHONY: ensure-has-wasm-toolchain ### Configures a wasm toolchain for test artifact building, if required
ensure-has-wasm-toolchain: target/wasm32-wasi/.obtained
target/wasm32-wasi/.obtained:
	@echo "# You should also install WABT for WASM module development!"
	@echo "# You can use your package manager or check https://github.com/WebAssembly/wabt"
	${MAYBE_ENVIRONMENT_EXEC} rustup target add wasm32-wasi
	@mkdir -p target/wasm32-wasi
	@touch target/wasm32-wasi/.obtained
