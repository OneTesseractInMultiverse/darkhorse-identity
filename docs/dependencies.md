# Dependency boundaries

Application lockfiles are committed. Update dependencies in a reviewed change, run the documented checks, and record compatibility/security findings. Dependency installation is separate from offline test execution. No claim of a complete dependency security audit is implied by a successful build.

- **Axum 0.8.9**, Serde, Tokio, and Tower are confined to HTTP/runtime adapters and composition. Transport rejections redact malformed request contents; request body limits remain explicit at consuming routes.
- **envbind 0.1.0** is pinned at the configuration adapter. Tests use `MapEnvironment`; complete settings, including library defaults, are validated before listener binding. Process environment access is restricted to the server/operator boundary; core logic receives explicit inputs. [API documentation](https://docs.rs/envbind/0.1.0/envbind/)
- **restqs 0.1.0** is pinned at the query adapter. The foundation example permits only a status filter and bounded pagination, rejects duplicate/unknown parameters, and maps into `DirectoryCriteria`. It is not a public directory endpoint or an authorization filter. Future SQL adapters must combine caller restrictions with independently established mandatory access predicates. [API documentation](https://docs.rs/restqs/0.1.0/restqs/)
- **SQLx 0.9.0** is confined to the PostgreSQL adapter (MIT/Apache-2.0). Enabled functionality is PostgreSQL, Tokio, rustls with ring, migrations/macros and UUIDs. Runtime parameterized queries avoid a compile-time database requirement. Only embedded migration macros are used; no query schema is fetched during unit compilation. SQL/client errors map to project-owned redacted failures. [SQLx documentation](https://docs.rs/sqlx/0.9.0/sqlx/)
- **RustCrypto argon2 0.6.0** (MIT/Apache-2.0), **getrandom 0.4**, **zeroize 1**, **uuid 1.26.1** and **rpassword 7.5.4** implement the password/entropy/terminal boundary. Argon2 uses allocation, PHC formatting and zeroization features. Hashing parameters and bounded worker admission are explicit; no custom cryptographic primitive is implemented. Exact transitive versions are locked. The selected memory/work profile exceeds the OWASP minimum but still needs workload-specific measurement for future login capacity. [Argon2 API](https://docs.rs/argon2/0.6.0/argon2/), [password storage guidance](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
- **Percona Distribution for PostgreSQL**, using Percona Server **18.6.1** based on PostgreSQL **18.6**, supplies the local Compose and integration database image, pinned by multi-platform digest. It uses the PostgreSQL License plus the licenses of bundled components. Preserve their notices; the container includes more than the database server. Rust, Node and Debian image stages are also pinned. Image updates require a reviewed digest change and compatibility/smoke checks. See the [container and migration contract](percona.md), [release notes](https://docs.percona.com/postgresql/18/release-notes/release-notes-v18.6.1.html), and [licensing information](https://docs.percona.com/postgresql/18/licensing.html).
- **SvelteKit**, **Svelte**, **Tailwind CSS**, and **shadcn-svelte** provide the static TypeScript UI. Exact resolved versions are in `pnpm-lock.yaml`. The button and utility source were installed using shadcn-svelte CLI 1.6.1. Styling uses local system fonts, with no remote font requirement.

## Redis infrastructure

Redis Open Source **8.10.1** is pinned by digest for independent cache and limiter instances. **redis-rs 1.7.0** (BSD-3-Clause) is pinned with default features disabled; enabled features are `tokio-rustls-comp` and `tls-rustls-webpki-roots`, and `script`. `percent-encoding` performs explicit credential comparison during configuration validation. Framework types stay in adapters. See [Redis infrastructure](redis.md) for actual setup, connection budgets and enforcement and recovery behavior. This feature review and successful testing are not a complete dependency advisory audit. [Redis client API](https://docs.rs/redis/1.7.0/redis/)

Redis 8 offers RSALv2, SSPLv1 and AGPLv3 licensing options; the project license/release decision remains separate and unresolved. Preserve applicable third-party notices when packaging the server. [Redis licenses](https://redis.io/legal/licenses/)

Cache and limiter use separate instances/configuration, identities, memory policies, deadlines, and recovery behavior. PostgreSQL remains authoritative. New authorization checks after a revocation commit must observe authoritative state before reusing versioned computation. Cache absence/corruption must be a miss; limiter uncertainty must reject affected attempts. Unit commands never provision Redis. Connection diagnostics do not grant admission. The shared limiter validates PostgreSQL authority before and after an atomic Redis charge; computation acceleration remains future work.

## Browser authentication additions

RustCrypto **sha2 0.11.0** and **hmac 0.13.0** provide session digests and keyed
login budget identifiers inside adapters. They were already transitive Rust
dependencies; direct versions are now explicit. No custom digest/MAC primitive is
implemented. **Playwright 1.63.0** is a root development dependency for disposable
Chromium integration. Browser downloads are explicit through `make browser-install`;
unit tests require no browser installation. These additions do not constitute a
complete advisory or license audit. See [authentication](authentication.md).

## Signing-key additions

**aws-lc-rs 1.18.1** (ISC/Apache-2.0; locked **aws-lc-sys 0.45.0**) supplies RSA-3072
key generation/PKCS#8 import, RS256 and AES-256-GCM. It stays inside adapters.
Default features are disabled; allocation, non-FIPS, prebuilt NASM and ring-compatible
public-key component access are explicit. This build does not claim FIPS certification.
The native dependency requires a C/C++ build toolchain; macOS command-line developer
tools and the pinned Rust Docker builder provide it. **base64 0.22.1** supplies
canonical unpadded URL-safe encoding, and **ring 0.17.14** is a direct development
dependency for independent RS256 verification. SHA-256 and OS entropy reuse existing
adapters. [AWS-LC API](https://docs.rs/aws-lc-rs/1.18.1/aws_lc_rs/),
[build requirements](https://aws.github.io/aws-lc-rs/requirements/index.html).

The five public AWS-LC Rust advisories reviewed on 2026-09-17 identify affected
`aws-lc-sys` ranges ending below 0.38.0 or 0.39.0; the resolved 0.45.0 is outside
those ranges. This targeted check is not a complete transitive dependency advisory
or license audit; no cargo-audit run is claimed. Recheck advisories when updating
or releasing. [Maintainer advisories](https://github.com/aws/aws-lc-rs/security/advisories).
