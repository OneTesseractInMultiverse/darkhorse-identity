# Darkhorse

<p>
  <img src="docs/brand/readme-banner.svg" alt="DarkHorse Identity Server" width="560">
</p>

An identity server for one organization and its applications, built with Rust, a static SvelteKit/TypeScript console, PostgreSQL, and Redis.

**Early development.** The repository contains a pure authorization policy engine, PostgreSQL persistence, operator-only administrator bootstrap, shared Redis attempt limiting, a password login portal with Rust-owned sessions, administrator-only application/client registration, protected signing keys/JWKS, and an initial `openid` authorization-code flow with consent, opaque access tokens, signed ID tokens, scoped UserInfo, client-authenticated introspection and revocation. Resource token issuance now binds persisted role permissions to immutable consent and token ceilings. Dedicated resource-server introspection now recomputes current capabilities for protected API checks. Opted-in confidential clients support [session-bound refresh rotation](docs/refresh-tokens.md) with family replay revocation and bounded cleanup. The portal supports [self-service session history and termination](docs/sessions.md) and an [administrator user directory](docs/console.md) with profile/name views, status changes, and application role assignments. The [application catalog console](docs/catalog-administration.md) manages clients, secrets, resources, scopes, capabilities, roles, and explicit bindings. Delegated administration, logout propagation and production deployment qualification remain unfinished.

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

For the optional database/bootstrap and Docker workflows, see [persistence](docs/persistence.md). Local Compose and integration tests use [Percona Distribution for PostgreSQL](docs/percona.md); that guide also covers existing-volume migration. `make test-postgres` and `make docker-smoke` use disposable infrastructure; ordinary unit tests remain service-free. Full coverage qualification remains open in [issue #2](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/2).

The [integrated Compose qualification stack](docs/compose.md) packages HTTPS, the Rust/static console image, PostgreSQL and separate TLS Redis services. It includes explicit setup, migration, bootstrap, health and backup commands, with mounted secrets and separate runtime/operator credentials. This initial topology is loopback-only and uses short-lived test certificates; production deployment qualification remains open.

The [Kubernetes application qualification](docs/kubernetes.md) adds explicit-context manifests, two restricted replicas, primary readiness, separate operator Jobs and an isolated local cluster test. Production failover, upgrades, restore and capacity qualification remain open.

Redis infrastructure, atomic shared attempt budgets and durable recovery are implemented in [issue #4](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/4). See [Redis setup, enforcement and recovery](docs/redis.md). For the enabled password portal, follow the [login setup and security contract](docs/authentication.md), then run `make dev-login`. The default preview keeps login disabled. `make browser-install` and `make test-browser` provide disposable HTTPS browser integration tests.

For opt-in code authorization and signing-key commands, see the [provider contract](docs/provider.md). `make provider-setup` and `make dev-provider` extend the prepared login environment; two-application SSO and [identity token checks](docs/token-checks.md) are exercised by `make test-browser`.

For opt-in current-email verification, see the [email verification and SMTP contract](docs/email-verification.md). `make email-setup` preserves a private local verification key; `make dev-email` starts the HTTPS login portal with your configured SMTP service. The full browser suite exercises actual TLS email delivery and single-use confirmation.

For a reproducible release-build HTTPS workload, run `make benchmark` or `make benchmark-baseline`. Use `make benchmark-arrivals` or `make benchmark-arrivals-baseline` for paced arrivals that continue independently of response times. Use `make benchmark-profile` or `make benchmark-profile-baseline` for opt-in Rust stage/pool and SQL/WAL profiling. Use `make benchmark-pools` or `make benchmark-pools-baseline` for sequential, repeated connection-pool comparisons. See the [performance baseline](docs/performance.md) for raw reports, revocation checks, topology and measurement limits.

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
| `deploy`             | Compose/Kubernetes packaging and reviewed runtime grants            |

Tests mirror source paths under `tests/unit`. Rust includes private unit modules from that parallel tree. Framework and serialization types remain outside domain/application contracts. See [engineering](docs/engineering.md) for the implementation rules and [dependencies](docs/dependencies.md) for library boundaries.

The [resource issuance contract](docs/resource-issuance.md) describes persisted role assignments and bounded policy loading. The [resource introspection contract](docs/resource-introspection.md) covers dedicated resource credentials and live capability checks. [Personal API keys](docs/personal-api-keys.md) add owner-managed, application-bound credentials with immutable resource ceilings, optional expiration, and single-reveal delivery at `/security/keys`. The [authorization contract](docs/authorization.md) explains application isolation, roles, scopes, credential ceilings, and authoritative-state requirements. Run `make test-authorization` and `make test-personal-keys` for their self-contained tests.

## Work tracking

Contributions follow an issue → feature branch → reviewed pull request workflow. Every new commit references its issue. Start with [Contributing](CONTRIBUTING.md) and the [engineering rules](docs/engineering.md).

Remaining work is tracked in [GitHub issues](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues). Persistence and bootstrap are tracked in [issue #3](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/3). Application and confidential client registration is tracked in [issue #6](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/6). See the [registration API and security contract](docs/registration.md), including exact callbacks, explicit resource/scope allowances and one-time client-secret issuance.

The project license has not been selected. Third-party component licenses remain applicable; see [third-party notices](docs/third-party-notices.md).

The [Rust CLI guide](docs/cli.md) documents command groups, protected input, confirmations, JSON output and exit codes. Use `make cli-help` to discover existing operations and read their [current authority boundaries](docs/operator-authority.md).

See [release qualification](docs/release-readiness.md) for hosted checks, advisory scans and remaining production gates. Report suspected vulnerabilities through the private channel in [SECURITY.md](SECURITY.md).

Invitation-only ordinary-account onboarding and its API are described in [the invitation contract](docs/invitations.md).

See [profiles, private images, and login branding](docs/profiles-and-media.md) for the account/settings screens and local object-storage commands.
