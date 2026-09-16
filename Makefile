.DEFAULT_GOAL := help
SHELL := /bin/bash
.DELETE_ON_ERROR:

PNPM ?= pnpm
NODE ?= node
CADDY ?= caddy
TEST_FILTER ?=
MUTATION_JOBS ?= 2
WEB := $(PNPM) --filter @darkhorse/console

.PHONY: help doctor deps-install deps-check fmt fmt-check lint typecheck architecture-check check ci test test-unit test-unit-rust test-unit-web test-tooling test-component test-unit-watch build build-api build-web dev-setup dev dev-api dev-web proxy-up https-setup https-check https-trust https-untrust clean
.PHONY: coverage-unit coverage-rust coverage-web
.PHONY: test-authorization test-property test-mutation

help: ## Help: list implemented targets; no setup required
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z_-]+:.*## / { printf "  %-23s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@printf '\nVariables: PNPM=pnpm NODE=node CADDY=caddy TEST_FILTER=<test-name> MUTATION_JOBS=2\n'
	@printf 'Examples: make deps-install; make check; make https-setup; make dev\n'
	@printf 'Tests need Rust + Node + pnpm. HTTPS development also needs Caddy 2.11.4.\n'

doctor: ## Setup: read-only toolchain and optional development diagnostics
	@rustc --version
	@cargo --version
	@$(NODE) --version
	@$(PNPM) --version
	@"$(CADDY)" version || printf 'Optional: install Caddy 2.11.4 for HTTPS development.\n'
	@docker --version 2>/dev/null || printf 'Optional: Docker is used by later persistence/deployment work.\n'

deps-install: ## Setup: fetch locked dependencies (network required)
	cargo fetch --locked
	$(PNPM) install --frozen-lockfile

deps-check: ## Check: verify lockfiles using installed dependencies
	cargo metadata --locked --offline --no-deps --format-version 1 > /dev/null
	$(PNPM) install --frozen-lockfile --offline --ignore-scripts

fmt: ## Style: format Rust, frontend, scripts, and public documentation
	cargo fmt --all
	$(WEB) exec prettier --write . ../../scripts ../../docs ../../README.md ../../package.json ../../pnpm-workspace.yaml

fmt-check: ## Style: verify formatting without edits
	cargo fmt --all -- --check
	$(WEB) exec prettier --check . ../../scripts ../../docs ../../README.md ../../package.json ../../pnpm-workspace.yaml

lint: ## Check: Rust Clippy and frontend ESLint
	cargo clippy --workspace --all-targets --locked --offline -- -D warnings
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

build: build-api build-web ## Build: Rust release binary and static console

build-api: ## Build: release Rust server (no network after dependency installation)
	cargo build --release --locked --offline -p darkhorse-server

build-web: ## Build: static SvelteKit console, with no runtime Node server
	$(WEB) build

dev-setup: https-setup ## Develop: prepare local CA without changing host trust

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
