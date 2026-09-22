# Dependency boundaries

Application lockfiles are committed. Update dependencies in a reviewed change, run the documented checks, and record compatibility/security findings. Dependency installation is separate from offline test execution. No claim of a complete dependency security audit is implied by a successful build.

- **Axum 0.8.9**, Serde, Tokio, and Tower are confined to HTTP/runtime adapters and composition. Transport rejections redact malformed request contents; request body limits remain explicit at consuming routes.
- **envbind 0.1.0** is pinned at the configuration adapter. Tests use `MapEnvironment`; complete settings, including library defaults, are validated before listener binding. Process environment access is restricted to the server/operator boundary; core logic receives explicit inputs. [API documentation](https://docs.rs/envbind/0.1.0/envbind/)
- **restqs 0.1.0** is pinned at the query adapter. The foundation example permits only a status filter and bounded pagination, rejects duplicate/unknown parameters, and maps into `DirectoryCriteria`. It is not a public directory endpoint or an authorization filter. Future SQL adapters must combine caller restrictions with independently established mandatory access predicates. [API documentation](https://docs.rs/restqs/0.1.0/restqs/)
- **SQLx 0.9.0** is confined to the PostgreSQL adapter (MIT/Apache-2.0). Enabled functionality is PostgreSQL, Tokio, rustls with ring, migrations/macros and UUIDs. Runtime parameterized queries avoid a compile-time database requirement. Only embedded migration macros are used; no query schema is fetched during unit compilation. SQL/client errors map to project-owned redacted failures. [SQLx documentation](https://docs.rs/sqlx/0.9.0/sqlx/)
- **RustCrypto argon2 0.6.0** (MIT/Apache-2.0), **getrandom 0.4**, **zeroize 1**, **uuid 1.26.1** and **rpassword 7.5.4** implement the password/entropy/terminal boundary. Argon2 uses allocation, PHC formatting and zeroization features. Hashing parameters and bounded worker admission are explicit; no custom cryptographic primitive is implemented. Exact transitive versions are locked. The selected memory/work profile exceeds the OWASP minimum but still needs workload-specific measurement for future login capacity. [Argon2 API](https://docs.rs/argon2/0.6.0/argon2/), [password storage guidance](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
- **Percona Distribution for PostgreSQL**, using Percona Server **18.6.1** based on PostgreSQL **18.6**, supplies the local Compose and integration database image, pinned by multi-platform digest. It uses the PostgreSQL License plus the licenses of bundled components. Preserve their notices; the container includes more than the database server. Rust, Node and Debian image stages are also pinned. Image updates require a reviewed digest change and compatibility/smoke checks. See the [container and migration contract](percona.md), [release notes](https://docs.percona.com/postgresql/18/release-notes/release-notes-v18.6.1.html), and [licensing information](https://docs.percona.com/postgresql/18/licensing.html).
- **Caddy 2.11.4** supplies the pinned Alpine proxy image for [Compose qualification](compose.md). The derived image removes the executable's privileged-port capability; TLS keys and trust material arrive through explicit secret mounts. Its [host matcher](https://caddyserver.com/docs/caddyfile/matchers#host) and [fallback handler](https://caddyserver.com/docs/caddyfile/directives/handle) reject requests for other hostnames. The proxy contains no identity policy or application session logic.
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

## Email delivery additions

**Lettre 0.11.23** (MIT) stays in the SMTP adapter. Defaults are disabled; the
selected features are `builder`, `smtp-transport`, `tokio1-rustls`, `ring`, and
`webpki-roots`. There is no sendmail, tracing, connection pool or Boring TLS
backend. SMTP uses implicit TLS with certificate and hostname validation; an
optional explicit private CA adds a trust anchor. SHA-256, HMAC, OS entropy and
zeroization reuse existing dependencies. [Pinned API](https://docs.rs/lettre/0.11.23/lettre/).

The three published Lettre RustSec advisories were reviewed on 2026-09-19.
The pinned version is outside their affected ranges: the Boring TLS hostname
issue is fixed from 0.11.22 (and does not affect rustls); the older SMTP-body and
sendmail-injection issues were fixed before 0.11. This targeted review is not a
complete transitive advisory or license audit. [Lettre advisories](https://rustsec.org/packages/lettre.html).

**rustls-pki-types 1.15.1** (MIT/Apache-2.0; already present transitively) is now an explicit adapter dependency for strict single-certificate PEM parsing. Empty or multi-certificate custom trust inputs are rejected before TLS configuration.

## Profile and image dependencies

- `isocountry` 0.3.2 (MIT) supplies the compiled ISO country list; `phonenumber` 0.3.10 (Apache-2.0, libphonenumber 9.0.33 metadata) validates canonical international contact numbers. [Country API](https://docs.rs/isocountry/0.3.2/isocountry/), [phone API](https://docs.rs/phonenumber/0.3.10+9.0.33/phonenumber/).
- `image` 0.25.10 (MIT/Apache-2.0), default features disabled, enables only PNG and JPEG decoding/encoding. Width/height limits are strict; allocation limits are best effort. [Decoder limits](https://docs.rs/image/0.25.10/image/struct.Limits.html).
- Apache `object_store` 0.14.2 (MIT/Apache-2.0), default features disabled with S3/TLS support, supplies signed object requests; `futures-util` provides bounded stream consumption. Exact transitive dependencies are locked. [Object store API](https://docs.rs/object_store/0.14.2/object_store/).
- RustFS 1.0.0 (Apache-2.0) is the pinned local/disposable S3-compatible service; it is not the required production provider. [Upstream release](https://github.com/rustfs/rustfs/releases/tag/1.0.0).

## Local Kubernetes verifier

[kind 0.33.0](https://github.com/kubernetes-sigs/kind/releases/tag/v0.33.0) is an optional host-only integration-test dependency. Verify its official release checksum before installation. The harness pins `kindest/node:v1.36.4@sha256:099e049362a1526b2db71494e1947aae99bd16290d7c895f2b7ea312e3cbfaed` and uses explicit private kubeconfig/context arguments. The [pinned kindnet source](https://github.com/kubernetes-sigs/kind/blob/v0.33.0/images/kindnetd/cmd/kindnetd/main.go) enables a network-policy controller with fail-open evaluation errors. Normal-path enforcement in this local fixture does not qualify production CNI failure behavior. Kubernetes API validation was exercised with kubectl 1.36.1. See [Kubernetes qualification](kubernetes.md) for topology and limits.

## CLI interface dependency

Clap **4.6.7** (MIT OR Apache-2.0) is pinned in the adapter layer with defaults
disabled and only `derive`, `std`, `help` and `usage` enabled. Color, suggestions,
environment-backed arguments and rich error context are disabled. The CLI drops
raw parser errors and emits fixed diagnostics. The new locked packages are
`clap`, `clap_builder` and `clap_derive` 4.6.7, `clap_lex` 1.1.1, and `anstyle`
1.0.14; all use MIT OR Apache-2.0. Their declared Rust minimums (1.85 for Clap and
1.66 for anstyle) are below the pinned 1.97.1 toolchain. This review uses package
metadata and the [maintained upstream API](https://docs.rs/clap/4.6.7/clap/).

The 2026-09-21 full-lockfile cargo-audit 0.22.2 check introduced no new findings;
the existing `atomic-polyfill` warning in #28 still blocks overall qualification.
This is maintenance/feature/license/advisory evidence, not an independent audit.
Existing rpassword 7.5.4 still owns hidden terminal reading; its collection-time
allocation and signal-restoration limits are explicit in [the CLI guide](cli.md).

## Release advisory review

The first complete-lockfile scan on **2026-09-21** found `cookie 0.6.0` through
SvelteKit 2.70.3. The scoped workspace override `@sveltejs/kit>cookie: 0.7.2`
retains the parse/serialize API and fixes
[GHSA-pxg6-pf52-xh8x](https://github.com/advisories/GHSA-pxg6-pf52-xh8x).
Rust owns Darkhorse's production sessions/cookies; this JavaScript dependency
belongs to build/development tooling. It still receives security maintenance.
Review removal of the override when SvelteKit declares a patched dependency.
The post-update pnpm 11.19.0 scan reported no JavaScript advisories; this is dated
evidence, not a permanent claim. [Upstream release](https://github.com/jshttp/cookie/releases/tag/v0.7.2).

cargo-audit **0.22.2** reported no Rust vulnerability advisories and one
unmaintained-package warning:
[RUSTSEC-2023-0089](https://rustsec.org/advisories/RUSTSEC-2023-0089.html),
`atomic-polyfill 1.0.3`, pulled into the lockfile by
`phonenumber → postcard → heapless`. The `cargo tree --target all --invert
atomic-polyfill` graph exposes that chain. Inverse graphs for
`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, and
`aarch64-apple-darwin` contain no active dependency; heapless selects it for
embedded targets. This limits the observed runtime exposure but does not turn a
full-lockfile warning into a passing release check. The strict gate remains
blocked, with no ignore or target filter. [Issue #28](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/28)
tracks removing the obsolete dependency before claiming complete dependency qualification.

Run `make audit-tools` explicitly to install the pinned auditor, and
`make audit-dependencies` for fresh evidence. The committed `.cargo/audit.toml`
keeps all informational warnings, rejects stale database use and sets no ignores.
The wrapper validates report shape/counts, captures scanner exit status and
projects only public dependency findings into the report. See
[release qualification](release-readiness.md) for report scope and remaining
image/license/provenance checks. [cargo-audit source](https://github.com/rustsec/rustsec/tree/cargo-audit/v0.22.2/cargo-audit).

Hosted checks pin `actions/checkout` 7.0.1, `actions/setup-node` 7.0.0 and
`actions/upload-artifact` 7.0.1 to verified upstream commit references. Updates
require reviewing upstream provenance and permissions; a tag comment is not a
substitute for the immutable reference.
