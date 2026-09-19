.DEFAULT_GOAL := help
SHELL := /bin/bash
.DELETE_ON_ERROR:

PNPM ?= pnpm
NODE ?= node
CADDY ?= caddy
TEST_FILTER ?=
MUTATION_JOBS ?= 2
IMAGE ?= darkhorse:local
WEB := $(PNPM) --filter @darkhorse/console

.PHONY: help doctor deps-install deps-check fmt fmt-check lint typecheck architecture-check check ci test test-unit test-unit-rust test-unit-web test-tooling test-component test-unit-watch build build-api build-web dev-setup dev dev-api dev-web proxy-up https-setup https-check https-trust https-untrust clean
.PHONY: coverage-unit coverage-rust coverage-web coverage-postgres
.PHONY: test-authorization test-property test-mutation
.PHONY: db-setup db-up db-down db-migrate bootstrap test-postgres docker-build docker-smoke
.PHONY: redis-setup redis-up redis-down redis-status test-redis docker-redis-smoke
.PHONY: redis-acl-update limiter-fence limiter-activate limiter-status
.PHONY: test-limiting test-mutation-limiting test-mutation-recovery coverage-core coverage-integration
.PHONY: login-setup dev-login browser-install test-browser
.PHONY: test-registration test-refresh

test-refresh: ## Test: isolated refresh policy, credential material and HTTP contracts
	cargo test --workspace --lib --locked --offline refresh

test-registration: ## Test: isolated application/client registration policies and transport
	cargo test --workspace --lib --locked --offline registration

browser-install: ## Test: install the pinned Chromium browser for integration tests
	$(PNPM) exec playwright install chromium

test-browser: build-web ## Test: disposable PostgreSQL/Redis and verified HTTPS login, SSO and token checks
	DARKHORSE_TEST_BROWSER=true $(NODE) scripts/redis-test.mjs

help: ## Help: list implemented targets; no setup required
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z_-]+:.*## / { printf "  %-23s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@printf '\nVariables: PNPM=pnpm NODE=node CADDY=caddy IMAGE=darkhorse:local TEST_FILTER=<test-name> MUTATION_JOBS=2\n'
	@printf 'Examples: make deps-install; make check; make https-setup; make dev\n'
	@printf 'Tests need Rust + Node + pnpm. HTTPS development also needs Caddy 2.11.4.\n'

doctor: ## Setup: read-only toolchain and optional development diagnostics
	@rustc --version
	@cargo --version
	@$(NODE) --version
	@$(PNPM) --version
	@"$(CADDY)" version || printf 'Optional: install Caddy 2.11.4 for HTTPS development.\n'
	@docker --version 2>/dev/null || printf 'Optional: Docker is needed for PostgreSQL integration and image commands.\n'

deps-install: ## Setup: fetch locked dependencies (network required)
	cargo fetch --locked
	$(PNPM) install --frozen-lockfile

deps-check: ## Check: verify lockfiles using installed dependencies
	cargo metadata --locked --offline --no-deps --format-version 1 > /dev/null
	$(PNPM) install --frozen-lockfile --offline --ignore-scripts

fmt: ## Style: format Rust, frontend, scripts, and public documentation
	cargo fmt --all
	$(WEB) exec prettier --write . ../../scripts ../../docs ../../.github ../../README.md ../../CONTRIBUTING.md ../../package.json ../../pnpm-workspace.yaml

fmt-check: ## Style: verify formatting without edits
	cargo fmt --all -- --check
	$(WEB) exec prettier --check . ../../scripts ../../docs ../../.github ../../README.md ../../CONTRIBUTING.md ../../package.json ../../pnpm-workspace.yaml

lint: ## Check: Rust Clippy and frontend ESLint
	cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
	$(WEB) lint

typecheck: ## Check: strict TypeScript and Svelte diagnostics
	$(WEB) check

architecture-check: ## Check: inward crate dependencies and static frontend boundaries
	$(NODE) scripts/architecture-check.mjs

check: fmt-check lint typecheck architecture-check test-unit ## Check: complete fast verification; no services or certificate setup

ci: check build ## Check: fast verification plus release/static builds

test: test-unit ## Test: alias for isolated unit suites

test-unit: test-unit-rust test-unit-web test-tooling ## Test: all isolated tests; no services, settings, or browser install

test-unit-rust: ## Test: Rust in-memory unit modules; optional TEST_FILTER
	cargo test --workspace --lib --locked --offline $(TEST_FILTER)

test-authorization: ## Test: pure authorization contracts and exhaustive set properties
	cargo test -p darkhorse-domain --lib --locked --offline authorization::

test-property: ## Test: deterministic exhaustive authorization properties
	cargo test -p darkhorse-domain --lib --locked --offline authorization::properties::

test-limiting: ## Test: pure attempt budgets and fail-closed admission contracts
	cargo test -p darkhorse-domain -p darkhorse-application --lib --locked --offline limiting::
	cargo test -p darkhorse-domain --lib --locked --offline limiter_recovery::

test-mutation-recovery: ## Test: durable fencing/time mutations; requires cargo-mutants 27.1.0
	cargo mutants --no-config -p darkhorse-domain --file 'crates/domain/src/limiter_recovery.rs' --cargo-arg=--locked --cargo-arg=--offline --cargo-test-arg=--lib --jobs $(MUTATION_JOBS) --timeout 30 --output target/mutation-recovery

test-mutation-limiting: ## Test: attempt-budget mutations; requires cargo-mutants 27.1.0
	cargo mutants --no-config -p darkhorse-domain --file 'crates/domain/src/limiting.rs' --cargo-arg=--locked --cargo-arg=--offline --cargo-test-arg=--lib --jobs $(MUTATION_JOBS) --timeout 30 --output target/mutation-limiting

test-mutation: ## Test: authorization mutations; requires cargo-mutants 27.1.0
	cargo mutants --no-config -p darkhorse-domain --file 'crates/domain/src/authorization/*.rs' --cargo-arg=--locked --cargo-arg=--offline --cargo-test-arg=--lib --jobs $(MUTATION_JOBS) --timeout 30 --output target/mutation

test-unit-web: ## Test: frontend computations and component interactions in memory
	$(WEB) test:unit $(TEST_FILTER)

test-tooling: ## Test: architecture and process-supervision contracts with fakes
	$(NODE) --test scripts/tests/unit/*.test.mjs

test-component: test-unit-web ## Test: self-contained UI component suite

test-unit-watch: ## Test: watch frontend unit/component tests; optional TEST_FILTER
	$(WEB) test:watch $(TEST_FILTER)

coverage-unit: coverage-rust coverage-web ## Coverage: report Rust library and frontend unit scopes separately

coverage-rust: ## Coverage: Rust libraries; requires cargo-llvm-cov 0.9.1 + llvm-tools-preview
	cargo llvm-cov --workspace --lib --locked --offline --summary-only --fail-under-lines 100

coverage-web: ## Coverage: frontend computations/components; see documented denominator
	$(WEB) coverage

coverage-postgres: ## Coverage: combine Rust unit, real PostgreSQL and CLI execution; requires Docker
	NODE="$(NODE)" bash scripts/coverage-postgres.sh

coverage-core: ## Coverage: framework-free domain/application only; not overall coverage
	cargo llvm-cov -p darkhorse-domain -p darkhorse-application --lib --locked --offline --summary-only --fail-under-lines 100

coverage-integration: ## Coverage: Rust unit + PostgreSQL + Redis + operator CLI; requires Docker
	NODE="$(NODE)" bash scripts/coverage-integration.sh

build: build-api build-web ## Build: Rust release binary and static console

build-api: ## Build: release Rust server (no network after dependency installation)
	cargo build --release --locked --offline -p darkhorse-server

build-web: ## Build: static SvelteKit console, with no runtime Node server
	$(WEB) build

db-setup: ## Database: generate owner-only local credentials; preserve existing files
	$(NODE) scripts/database.mjs setup

db-up: ## Database: start project-local PostgreSQL with loopback-only access
	$(NODE) scripts/database.mjs up

db-down: ## Database: stop this workspace's database; preserve its volume and credentials
	$(NODE) scripts/database.mjs down

db-migrate: ## Database: explicitly apply embedded migrations to the local database
	$(NODE) scripts/database.mjs run migrate

bootstrap: ## Database: interactively create the one-time local administrator
	$(NODE) scripts/database.mjs run bootstrap

test-postgres: ## Test: disposable PostgreSQL transactions and operator CLI; requires Docker
	$(NODE) scripts/postgres-test.mjs

docker-build: ## Build: pinned multi-stage Rust/static image; requires network on first build
	docker build --tag "$(IMAGE)" .

docker-smoke: ## Test: built image, temporary database, bootstrap and HTTP/static behavior
	$(NODE) scripts/postgres-test.mjs --image "$(IMAGE)"

docker-redis-smoke: ## Test: packaged Redis diagnostics against isolated disposable services
	$(NODE) scripts/redis-test.mjs --image "$(IMAGE)"

redis-setup: ## Redis: generate separate local role credentials and protected ACL files
	$(NODE) scripts/redis.mjs setup

redis-up: ## Redis: start isolated cache and limiter services on loopback ports
	$(NODE) scripts/redis.mjs up

redis-down: ## Redis: stop owned services; preserve limiter data and credentials
	$(NODE) scripts/redis.mjs down

redis-status: ## Redis: inspect each connection and memory policy; not an enforcement probe
	$(NODE) scripts/redis.mjs status

redis-acl-update: ## Redis: update local ACL policy while retaining all existing credentials
	$(NODE) scripts/redis.mjs acl-update

limiter-fence: ## Limiter: durably stop admission and begin the mandatory recovery wait
	$(NODE) scripts/redis.mjs limiter-fence

limiter-activate: ## Limiter: initialize a waited generation using protected operator credentials
	$(NODE) scripts/redis.mjs limiter-activate

limiter-status: ## Limiter: inspect durable generation, recovery wait and counter state
	$(NODE) scripts/redis.mjs limiter-status

test-redis: ## Test: disposable Redis processes and Rust infrastructure diagnostics
	$(NODE) scripts/redis-test.mjs

dev-setup: https-setup ## Develop: prepare local CA without changing host trust

login-setup: ## Develop: generate a private local login limiter key; preserve an existing key
	$(NODE) scripts/login.mjs setup

dev-login: ## Develop: full HTTPS password portal using prepared local database/Redis/key
	CADDY="$(CADDY)" $(NODE) scripts/login.mjs dev

dev: ## Develop: Rust reload, frontend HMR, and HTTPS proxy; Ctrl-C stops owned processes
	CADDY="$(CADDY)" $(NODE) scripts/dev.mjs all

dev-api: ## Develop: Rust with reload on source changes; loopback port 3001
	$(NODE) scripts/dev.mjs api

dev-web: ## Develop: frontend asset/HMR server; loopback port 5173
	$(WEB) dev

proxy-up: ## HTTPS: foreground Caddy proxy; Ctrl-C stops it
	CADDY="$(CADDY)" $(NODE) scripts/dev.mjs proxy

https-setup: ## HTTPS: validate proxy and prepare project CA; does not install trust
	CADDY="$(CADDY)" $(NODE) scripts/dev.mjs setup

https-check: ## HTTPS: verify project CA, localhost hostname, and Rust health route
	$(NODE) scripts/dev.mjs check

https-trust: ## HTTPS: explicitly trust this project's CA in the macOS user keychain
	bash scripts/trust.sh trust

https-untrust: ## HTTPS: explicitly remove this project's CA trust from macOS
	bash scripts/trust.sh untrust

clean: ## Clean: delete only reproducible outputs; preserve dependencies and local CA
	cargo clean
	rm -rf apps/console/.svelte-kit apps/console/build apps/console/coverage coverage

.PHONY: provider-setup dev-provider signing-status signing-generate signing-import signing-activate signing-retire test-provider

provider-setup: ## Provider: generate owner-only local wrapping key; preserve existing key
	$(NODE) scripts/provider.mjs setup

dev-provider: ## Provider: HTTPS login and pending authorization using prepared local services
	CADDY="$(CADDY)" $(NODE) scripts/provider.mjs dev

signing-status: ## Provider: inspect public signing-key metadata and current revision
	$(NODE) scripts/provider.mjs run signing-status

signing-generate: ## Provider: generate and stage RSA-3072 key; requires REVISION
	$(NODE) scripts/provider.mjs run signing-generate "$(REVISION)"

signing-import: ## Provider: stage PKCS#8 DER from stdin; requires REVISION
	$(NODE) scripts/provider.mjs run signing-import --stdin "$(REVISION)"

signing-activate: ## Provider: activate a prepublished key; requires KID and REVISION
	$(NODE) scripts/provider.mjs run signing-activate "$(KID)" "$(REVISION)"

signing-retire: ## Provider: retire staged/expired retiring key; requires KID and REVISION
	$(NODE) scripts/provider.mjs run signing-retire "$(KID)" "$(REVISION)"

test-provider: ## Test: isolated authorization, signing, code exchange and identity-check contracts
	cargo test --workspace --lib --locked --offline oidc
	cargo test --workspace --lib --locked --offline signing
	cargo test --workspace --lib --locked --offline provider_http
	cargo test --workspace --lib --locked --offline tokens
	cargo test --workspace --lib --locked --offline token_http

.PHONY: test-resource-introspection

test-resource-introspection: ## Test: isolated resource credential lifecycle, transport and projections
	cargo test --workspace --lib --locked --offline resource_servers
	cargo test --workspace --lib --locked --offline token_http

BENCH_PROFILE ?= smoke
BENCH_POOL_SIZE ?= 5
.PHONY: benchmark benchmark-baseline

benchmark: build-web ## Performance: release HTTPS SSO/introspection baseline; disposable Docker services
	BENCH_PROFILE="$(BENCH_PROFILE)" BENCH_POOL_SIZE="$(BENCH_POOL_SIZE)" $(NODE) scripts/redis-test.mjs --benchmark

benchmark-baseline: ## Performance: larger bounded baseline with eight resource clients
	$(MAKE) benchmark BENCH_PROFILE=baseline

.PHONY: benchmark-arrivals benchmark-arrivals-baseline

benchmark-arrivals: ## Performance: short paced arrivals with bounded concurrency and missed-arrival reporting
	$(MAKE) benchmark BENCH_PROFILE=arrival-smoke

benchmark-arrivals-baseline: ## Performance: sustained paced arrivals and concurrent security changes
	$(MAKE) benchmark BENCH_PROFILE=arrival-baseline

.PHONY: benchmark-profile benchmark-profile-baseline

benchmark-profile: ## Performance: paced smoke with opt-in Rust stage/pool and PostgreSQL statement/WAL profiling
	$(MAKE) benchmark BENCH_PROFILE=profile-smoke

benchmark-profile-baseline: ## Performance: paced baseline with opt-in stage and SQL profiling
	$(MAKE) benchmark BENCH_PROFILE=profile-baseline

BENCH_POOL_PROFILE ?= arrival-smoke
.PHONY: benchmark-pools benchmark-pools-baseline

benchmark-pools: build-web ## Performance: sequential pool sizes 2/5/10/16, repeated in reverse order
	@set -eu; for pool in 2 5 10 16 16 10 5 2; do \
		BENCH_PROFILE="$(BENCH_POOL_PROFILE)" BENCH_POOL_SIZE="$$pool" $(NODE) scripts/redis-test.mjs --benchmark; \
	done

benchmark-pools-baseline: ## Performance: eight complete paced baseline runs with controlled pool sizes
	$(MAKE) benchmark-pools BENCH_POOL_PROFILE=arrival-baseline
