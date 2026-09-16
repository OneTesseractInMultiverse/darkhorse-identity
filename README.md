# Darkhorse

An identity server for one organization and its applications, built with Rust, a static SvelteKit/TypeScript console, PostgreSQL, and Redis.

**Early development.** The repository contains the workspace, a pure authorization policy engine, configuration/query adapters, a process liveness endpoint, and a console preview. Authentication, protocol endpoints, persistence, and deployment packaging are not implemented yet. The policy engine is not connected to HTTP requests.

## Quick start

Install Rust through rustup, Node **24.19.0**, pnpm **11.19.0**, and GNU Make **3.81 or newer**. The Rust toolchain is pinned to **1.97.1**. macOS and Linux are the intended development platforms; host certificate trust automation currently supports macOS.

```sh
make help
make deps-install
make check
make build
```

Dependency installation needs network access. After dependencies are installed, unit tests run without environment files, databases, Redis, Docker, certificates, or a browser installation. Rust commands use locked dependencies in offline mode. A clean checkout needs no private planning files.

For HTTPS development, install [Caddy 2.11.4](https://github.com/caddyserver/caddy/releases/tag/v2.11.4), then:

```sh
make https-setup
make https-trust  # explicit macOS user trust; may display an OS prompt
make dev
```

Open **https://localhost:8443**. Stop the foreground stack with Ctrl-C. Run `make https-check` in another terminal to validate the project CA and Rust route. See [development](docs/development.md) for trust, ports, and troubleshooting.

## Structure

| Directory            | Responsibility                                                        |
| -------------------- | --------------------------------------------------------------------- |
| `crates/domain`      | Framework-free identity types and authorization computations          |
| `crates/application` | Project-owned use-case inputs and future ports/coordinators           |
| `crates/adapters`    | Axum/Serde transport, envbind configuration, restqs query translation |
| `apps/server`        | Rust composition root and process lifecycle                           |
| `apps/console`       | Static SvelteKit frontend; TypeScript and shadcn-svelte               |
| `scripts`            | Development and repository verification tools                         |
| `config`             | Local HTTPS proxy configuration                                       |

Tests mirror source paths under `tests/unit`. Rust includes private unit modules from that parallel tree. Framework and serialization types remain outside domain/application contracts. See [engineering](docs/engineering.md) for the implementation rules and [dependencies](docs/dependencies.md) for library boundaries.

The [authorization contract](docs/authorization.md) explains application isolation, roles, scopes, credential ceilings, and the authoritative-state requirements for future adapters. Run `make test-authorization` for its self-contained tests.

The project license has not been selected. Third-party component licenses remain applicable; see [third-party notices](docs/third-party-notices.md).
