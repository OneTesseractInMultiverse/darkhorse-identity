# Darkhorse

An identity server for one organization and its applications, built with Rust, a static SvelteKit/TypeScript console, PostgreSQL, and Redis.

**Early development.** The repository contains the workspace, a pure authorization policy engine, configuration/query adapters, a process liveness endpoint, and a console preview. PostgreSQL persistence, operator-only administrator bootstrap, and a local image build are also implemented. Login, protocol endpoints, and production deployment qualification remain unfinished. The policy engine is not connected to HTTP requests.

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

For the optional database/bootstrap and Docker workflows, see [persistence](docs/persistence.md). `make test-postgres` and `make docker-smoke` use disposable infrastructure; ordinary unit tests remain service-free. Full coverage qualification remains open in [issue #2](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/2).

## Structure

| Directory            | Responsibility                                                      |
| -------------------- | ------------------------------------------------------------------- |
| `crates/domain`      | Framework-free identity types and authorization computations        |
| `crates/application` | Project-owned use cases, ports and coordinators                     |
| `crates/adapters`    | Transport, configuration, SQLx persistence and password preparation |
| `apps/server`        | Rust composition root and process lifecycle                         |
| `apps/console`       | Static SvelteKit frontend; TypeScript and shadcn-svelte             |
| `scripts`            | Development and repository verification tools                       |
| `config`             | Local HTTPS proxy and PostgreSQL Compose configuration              |

Tests mirror source paths under `tests/unit`. Rust includes private unit modules from that parallel tree. Framework and serialization types remain outside domain/application contracts. See [engineering](docs/engineering.md) for the implementation rules and [dependencies](docs/dependencies.md) for library boundaries.

The [authorization contract](docs/authorization.md) explains application isolation, roles, scopes, credential ceilings, and the authoritative-state requirements for future adapters. Run `make test-authorization` for its self-contained tests.

## Work tracking

Remaining work is tracked in [GitHub issues](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues). Development stays on `main`, with every new commit linked to its issue. See the [contribution workflow](docs/engineering.md#issue-based-work-on-main). Persistence and bootstrap are tracked in [issue #3](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/3). The next planned feature slice is [Redis infrastructure and distributed limiting](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/4).

The project license has not been selected. Third-party component licenses remain applicable; see [third-party notices](docs/third-party-notices.md).
