.PHONY: help
.DEFAULT_GOAL := help
_latest_version := $(shell scripts/version.sh true)
_version := $(shell scripts/version.sh)


help:
	@echo "                                      __   __  __"
	@echo "                                      \ \ / / / /"
	@echo "                                       \ V / / / "
	@echo "                                        \_/  \/  "
	@echo ""
	@echo "                                      V E C T O R"
	@echo ""
	@echo "---------------------------------------------------------------------------------------"
	@echo ""
	@awk 'BEGIN {FS = ":.*##"; printf "Usage: make \033[36m<target>\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-25s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Development

bench: ## Run internal benchmarks
	@cargo bench --all

build: ## Build the project
	@cargo build --no-default-features --features="$${FEATURES:-default}"

check: check-code check-fmt check-generate check-examples

check-code: ## Checks code for compilation errors (only default features)
	@cargo check --all --all-targets

check-fmt: ## Checks code formatting correctness
	@scripts/check-style.sh
	@cargo fmt -- --check

check-generate: ## Checks for pending `make generate` changes
	@bundle install --gemfile=scripts/Gemfile --quiet
	@scripts/check-generate.sh

check-examples: ## Validates the config examples
	@cargo run -q -- validate --topology --deny-warnings ./config/examples/*.toml

check-version: ## Checks that the version in Cargo.toml is up-to-date
	@bundle install --gemfile=scripts/Gemfile --quiet
	@scripts/check-version.rb

CHECK_URLS=false
export CHECK_URLS
generate: ## Generates files across the repo using the data in /.meta
	@bundle install --gemfile=scripts/Gemfile --quiet
	@scripts/generate.rb

fmt: ## Format code
	@scripts/check-style.sh --fix
	@cargo fmt

release: ## Release a new Vector version
	@$(MAKE) release-meta
	@$(MAKE) generate CHECK_URLS=false
	@$(MAKE) release-commit

run: ## Starts Vector in development mode
	@cargo run

signoff: ## Signsoff all previous commits since branch creation
	@scripts/signoff.sh

test: ## Spins up Docker resources and runs _every_ test
	@docker-compose up -d
	@cargo test --all --features docker -- --test-threads 4

clean: ## Remove build artifacts
	@cargo clean

##@ Releasing

build-archive: ## Build a Vector archive for a given $TARGET and $VERSION
	scripts/build-archive.sh

build-ci-docker-images: ## Build the various Docker images used for CI
	@scripts/build-ci-docker-images.sh

build-docker: ## Build the Vector docker images, but do not push
	@scripts/build-docker.sh

package-deb: ## Create a .deb package from artifacts created via `build`
	@scripts/package-deb.sh

package-rpm: ## Create a .rpm package from artifacts created via `build`
	@scripts/package-rpm.sh

release-commit: ## Commits release changes
	@scripts/release-commit.rb

release-docker: ## Release to Docker Hub
	@scripts/release-docker.sh

release-github: ## Release to Github
	@bundle install --gemfile=scripts/Gemfile --quiet
	@scripts/release-github.rb

release-homebrew: ## Release to timberio Homebrew tap
	@scripts/release-homebrew.sh

release-meta: ## Prepares the release metadata
	@bundle install --gemfile=scripts/Gemfile --quiet
	@scripts/release-meta.rb

release-rollback:
	@bundle install --gemfile=scripts/Gemfile --quiet
	@scripts/release-rollback.rb

release-s3: ## Release artifacts to S3
	@scripts/release-s3.sh

sync-install:
	@aws s3 cp distribution/install.sh s3://sh.vector.dev --sse --acl public-read

version: ## Get the current Vector version
	@echo $(_version)
