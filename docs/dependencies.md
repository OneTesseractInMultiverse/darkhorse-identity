# Dependency boundaries

Application lockfiles are committed. Update dependencies in a reviewed change, run the documented checks, and record compatibility/security findings. Dependency installation is separate from offline test execution. No claim of a complete dependency security audit is implied by a successful build.

- **Axum 0.8.9**, Serde, Tokio, and Tower are confined to HTTP/runtime adapters and composition. Transport rejections redact malformed request contents. Request body limits remain explicit at consuming routes.
- **envbind 0.1.0** is pinned at the configuration adapter. Tests use `MapEnvironment`. Complete settings, including library defaults, are validated before listener binding. Process environment access is restricted to the server/operator boundary. Core logic receives explicit inputs. [API documentation](https://docs.rs/envbind/0.1.0/envbind/)
- **restqs 0.1.1** stays in the query adapter, pinned with default features disabled. Administrative directory and catalog routes reuse its allowlisted status and limit parsing. The adapter handles literal search and keyset cursors separately. Query criteria never establish caller authority. Mandatory owner and administrator predicates come from authenticated use cases. The source-defined `DirectoryCriteria` example remains isolated from HTTP publication. See the [compatibility review](#query-parser-update) and [API documentation](https://docs.rs/restqs/0.1.1/restqs/).
- **SQLx 0.9.0** is confined to the PostgreSQL adapter (MIT/Apache-2.0). Selected functionality is PostgreSQL, Tokio, rustls with ring, migrations/macros and UUIDs. Runtime parameterized queries avoid a compile-time database requirement. Only embedded migration macros are used. No query schema is fetched during unit compilation. SQL/client errors map to project-owned redacted failures. [SQLx documentation](https://docs.rs/sqlx/0.9.0/sqlx/)
- **RustCrypto argon2 0.6.0** (MIT/Apache-2.0), **getrandom 0.4**, **zeroize 1**, and **uuid 1.26.1** implement the password/entropy boundary. Argon2 uses allocation, PHC formatting and zeroization features. Hashing parameters and bounded worker admission are explicit. No custom cryptographic primitive is implemented. Exact transitive versions are locked. The selected memory/work profile exceeds the OWASP minimum but still needs workload-specific measurement for future login capacity. [Argon2 API](https://docs.rs/argon2/0.6.0/argon2/), [password storage guidance](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
- **Percona Distribution for PostgreSQL**, using Percona Server **18.6.1** based on PostgreSQL **18.6**, supplies the local Compose and integration database image, pinned by multi-platform digest. It uses the PostgreSQL License plus the licenses of bundled components. Preserve their notices. The container includes more than the database server. Rust, Node and Debian image stages are pinned. Image updates require a reviewed digest change and compatibility/smoke checks. See the [container and migration contract](percona.md), [release notes](https://docs.percona.com/postgresql/18/release-notes/release-notes-v18.6.1.html), and [licensing information](https://docs.percona.com/postgresql/18/licensing.html).
- **Caddy 2.11.4** supplies the pinned Alpine proxy image for [Compose qualification](compose.md). The derived image removes the executable's privileged-port capability. TLS keys and trust material arrive through explicit secret mounts. Its [host matcher](https://caddyserver.com/docs/caddyfile/matchers#host) and [fallback handler](https://caddyserver.com/docs/caddyfile/directives/handle) reject requests for other hostnames. The proxy contains no identity policy or application session logic.
- **SvelteKit**, **Svelte**, **Tailwind CSS**, and **shadcn-svelte** provide the static TypeScript UI. Exact resolved versions are in `pnpm-lock.yaml`. The button and utility source were installed using shadcn-svelte CLI 1.6.1. Styling uses local system fonts, with no remote font requirement.

## Query parser update

Reviewed on **2026-09-24 UTC**: the published **restqs 0.1.1** archive matches
the registry checksum, and its Rust sources and original manifest match the
upstream release commit
[`973f216`](https://github.com/OneTesseractInMultiverse/restqs/tree/973f2167facc75727e67a19f25bcc819511798d7).
The crate is MIT-licensed, declares Rust 1.85, and has no package dependencies or
build script. The selected release needs no adapter API migration. Its optional
SQLx feature remains disabled; Darkhorse owns its parameterized SQL and authority
predicates. The lockfile changes only this package's version and checksum.
[Release notes](https://github.com/OneTesseractInMultiverse/restqs/releases/tag/v0.1.1).

The parser now enforces `max_value_bytes` for pagination values after percent
decoding. Darkhorse already configured a 32-byte limit, but 0.1.0 accepted longer
zero-padded values when their numeric result remained in range. Source-defined
regressions demonstrate that 32-byte values remain valid and 33-byte values fail,
both literally and percent-encoded. Administrative directory and catalog routes
return their fixed HTTP 400 errors without calling application services. The
2,048-byte raw query limit, page size range of 1–100, allowlisted status filters,
canonical nonzero cursors, and separate 100-character literal search limit remain.
Search text containing operator characters is still literal text.

The release also changes operator recognition, date validation, regex restrictions
and null SQL translation. Darkhorse exposes no date, regex, sort, projection or
arbitrary-field query interface, and consumes no generated SQL. Tests preserve
those rejections and accepted status/search/cursor behavior. Although upstream
improves error display redaction, error fields and debug formatting can retain
input. Darkhorse continues to discard parser errors at the adapter boundary and
uses project-owned failures and fixed transport messages. Do not log raw library
errors or query strings. This source and compatibility review supplements the
[complete-lockfile advisory check](#release-advisory-review); it does not establish
an independent security audit or a performance improvement.

## Redis infrastructure

Redis Open Source **8.10.1** is pinned by digest for independent cache and limiter instances. **redis-rs 1.7.0** (BSD-3-Clause) is pinned with default features disabled. Active features are `tokio-rustls-comp` and `tls-rustls-webpki-roots`, and `script`. `percent-encoding` performs explicit credential comparison during configuration validation. Framework types stay in adapters. See [Redis infrastructure](redis.md) for actual setup, connection budgets and enforcement and recovery behavior. This feature review and successful testing are not a complete dependency advisory audit. [Redis client API](https://docs.rs/redis/1.7.0/redis/)

Redis 8 offers RSALv2, SSPLv1 and AGPLv3 licensing options. The project license/release decision remains separate and unresolved. Preserve applicable third-party notices when packaging the server. [Redis licenses](https://redis.io/legal/licenses/)

Cache and limiter use separate instances/configuration, identities, memory policies, deadlines, and recovery behavior. PostgreSQL remains authoritative. New authorization checks after a revocation commit must observe authoritative state before reusing versioned computation. Cache absence/corruption must be a miss. Limiter uncertainty must reject affected attempts. Unit commands never provision Redis. Connection diagnostics do not grant admission. The shared limiter validates PostgreSQL authority before and after an atomic Redis charge. Computation acceleration remains future work.

## Browser authentication

RustCrypto **sha2 0.11.0** and **hmac 0.13.0** provide session digests and keyed
login budget identifiers inside adapters. They were already transitive Rust
dependencies. Direct versions are now explicit. No custom digest/MAC primitive is
implemented. **Playwright 1.63.0** is a root development dependency for disposable
Chromium integration. Browser downloads are explicit through `make browser-install`.
Unit tests require no browser installation. These additions do not constitute a
complete advisory or license audit. See [authentication](authentication.md).

## Signing keys

**aws-lc-rs 1.18.1** (ISC/Apache-2.0. Locked **aws-lc-sys 0.45.0**) supplies RSA-3072
key generation/PKCS#8 import, RS256 and AES-256-GCM. It stays inside adapters.
Default features are disabled. Allocation, non-FIPS, prebuilt NASM and ring-compatible
public-key component access are explicit. This build does not claim FIPS certification.
The native dependency requires a C/C++ build toolchain. MacOS command-line developer
tools and the pinned Rust Docker builder provide it. **base64 0.22.1** supplies
canonical unpadded URL-safe encoding, and **ring 0.17.14** is a direct development
dependency for independent RS256 verification. SHA-256 and OS entropy reuse existing
adapters. [AWS-LC API](https://docs.rs/aws-lc-rs/1.18.1/aws_lc_rs/),
[build requirements](https://aws.github.io/aws-lc-rs/requirements/index.html).

The current lockfile receives the complete advisory scan described below.
Review [maintainer advisories](https://github.com/aws/aws-lc-rs/security/advisories)
and rerun the scan for each update or release. A passing scan does not establish
an independent cryptographic audit or license review.

## Email delivery

**Lettre 0.11.23** (MIT) stays in the SMTP adapter. Defaults are disabled. The
selected features are `builder`, `smtp-transport`, `tokio1-rustls`, `ring`, and
`webpki-roots`. There is no sendmail, tracing, connection pool or Boring TLS
backend. SMTP uses implicit TLS with certificate and hostname validation. An
optional explicit private CA adds a trust anchor. SHA-256, HMAC, OS entropy and
zeroization reuse existing dependencies. [Pinned API](https://docs.rs/lettre/0.11.23/lettre/).

The three published Lettre RustSec advisories were reviewed on 2026-09-19.
The pinned version is outside their affected ranges: the Boring TLS hostname
issue is fixed from 0.11.22 (and does not affect rustls). The older SMTP-body and
sendmail-injection issues were fixed before 0.11. This targeted review is not a
complete transitive advisory or license audit. [Lettre advisories](https://rustsec.org/packages/lettre.html).

**rustls-pki-types 1.15.1** (MIT/Apache-2.0. Already present transitively) is now an explicit adapter dependency for strict single-certificate PEM parsing. Empty or multi-certificate custom trust inputs are rejected before TLS configuration.

## Profile and image dependencies

- `isocountry` 0.3.2 (MIT) supplies the compiled ISO country list. `rlibphonenumber` 2.2.12 (Apache-2.0, libphonenumber 9.0.39 metadata) validates canonical international contact numbers. [Country API](https://docs.rs/isocountry/0.3.2/isocountry/), [phone API](https://docs.rs/rlibphonenumber/2.2.12/rlibphonenumber/). See the [replacement review](#phone-validator-replacement).
- `image` 0.25.10 (MIT/Apache-2.0), default features disabled, activates only PNG and JPEG decoding/encoding. Width/height limits are strict. Allocation limits are best effort. [Decoder limits](https://docs.rs/image/0.25.10/image/struct.Limits.html).
- Apache `object_store` 0.14.2 (MIT/Apache-2.0), default features disabled with S3/TLS support, supplies signed object requests. `futures-util` provides bounded stream consumption. Exact transitive dependencies are locked. [Object store API](https://docs.rs/object_store/0.14.2/object_store/).
- RustFS 1.0.0 (Apache-2.0) is the pinned local/disposable S3-compatible service. It is not the required production provider. [Upstream release](https://github.com/rustfs/rustfs/releases/tag/1.0.0).

## Local Kubernetes verifier

[kind 0.33.0](https://github.com/kubernetes-sigs/kind/releases/tag/v0.33.0) is an optional host-only integration-test dependency. Verify its official release checksum before installation. The harness pins `kindest/node:v1.36.4@sha256:099e049362a1526b2db71494e1947aae99bd16290d7c895f2b7ea312e3cbfaed` and uses explicit private kubeconfig/context arguments. The [pinned kindnet source](https://github.com/kubernetes-sigs/kind/blob/v0.33.0/images/kindnetd/cmd/kindnetd/main.go) activates a network-policy controller with fail-open evaluation errors. Normal-path enforcement in this local fixture does not qualify production CNI failure behavior. Kubernetes API validation was exercised with kubectl 1.36.1. See [Kubernetes qualification](kubernetes.md) for topology and limits.

## CLI interface dependency

Clap **4.6.7** (MIT OR Apache-2.0) is pinned in the adapter layer with defaults
disabled and only `derive`, `std`, `help` and `usage` active. Color, suggestions,
environment-backed arguments and rich error context are disabled. The CLI drops
raw parser errors and emits fixed diagnostics. The new locked packages are
`clap`, `clap_builder` and `clap_derive` 4.6.7, `clap_lex` 1.1.1, and `anstyle`
1.0.14. All use MIT OR Apache-2.0. Their declared Rust minimums (1.85 for Clap and
1.66 for anstyle) are below the pinned 1.97.1 toolchain. This review uses package
metadata and the [maintained upstream API](https://docs.rs/clap/4.6.7/clap/).

The [current advisory review](#release-advisory-review) covers the complete lockfile.
Package metadata and tests do not replace an independent security audit.
The terminal adapter replaces rpassword/rtoolbox with **nix 0.31.3** (MIT,
Rust minimum 1.69), using only `term`, `process`, `signal` and `fs` with defaults
disabled. Its added build dependency is `cfg_aliases` 0.2.2 (MIT). `libc`, `bitflags`
and `cfg-if` retain their existing locked versions. Safe APIs provide terminal
settings, descriptor metadata and foreground-group checks. The project adds no
unsafe code. See the [nix terminal API](https://docs.rs/nix/0.31.3/nix/sys/termios/).

Existing Tokio 1.53.1 supplies nonblocking descriptor readiness and catchable
signal streams. Its signal handlers persist for the process, so they are installed
only for interactive bootstrap and account commands, through each operation. No signal
registration is added to help, parsing, protected-stdin operations or ordinary
server startup. See [descriptor readiness](https://docs.rs/tokio/1.53.1/tokio/io/unix/struct.AsyncFd.html)
and [signal lifetime](https://docs.rs/tokio/1.53.1/tokio/signal/unix/struct.Signal.html).
Terminal restoration and abort/device-loss boundaries are explicit in
[the CLI guide](cli.md). The phone-validator replacement removed the historical
unmaintained-package finding.

## Release advisory review

The first complete-lockfile scan on **2026-09-21** found `cookie 0.6.0` through
SvelteKit 2.70.3. The scoped workspace override `@sveltejs/kit>cookie: 0.7.2`
retains the parse/serialize API and fixes
[GHSA-pxg6-pf52-xh8x](https://github.com/advisories/GHSA-pxg6-pf52-xh8x).
Rust owns Darkhorse's production sessions/cookies. This JavaScript dependency
belongs to build/development tooling. It still receives security maintenance.
Review removal of the override when SvelteKit declares a patched dependency.
The post-update pnpm 11.19.0 scan reported no JavaScript advisories. This is dated
evidence, not a permanent claim. [Upstream release](https://github.com/jshttp/cookie/releases/tag/v0.7.2).

The historical 2026-09-21 cargo-audit **0.22.2** scan reported one
unmaintained-package warning and no Rust vulnerability advisory:
[RUSTSEC-2023-0089](https://rustsec.org/advisories/RUSTSEC-2023-0089.html),
`atomic-polyfill 1.0.3`, pulled into the lockfile by
`phonenumber → postcard → heapless`. The `cargo tree --target all --invert
atomic-polyfill` graph exposes that chain. Inverse graphs for
`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, and
`aarch64-apple-darwin` contain no active dependency. Heapless selects it for
embedded targets. This limits the observed runtime exposure but does not turn a
full-lockfile warning into a passing release check. That scan remained blocked,
with no ignore or target filter. [Issue #28](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/28)
records the replacement below and revision-specific verification evidence.

Run `make audit-tools` explicitly to install the pinned auditor, and
`make audit-dependencies` for fresh evidence. The committed `.cargo/audit.toml`
keeps all informational warnings, rejects stale database use and sets no ignores.
The wrapper validates report shape/counts, captures scanner exit status and
projects only public dependency findings into the report. See
[release qualification](release-readiness.md) for report scope and remaining
image/license/provenance checks. [cargo-audit source](https://github.com/rustsec/rustsec/tree/cargo-audit/v0.22.2/cargo-audit).

Hosted checks pin `actions/checkout` 7.0.1, `actions/setup-node` 7.0.0 and
`actions/upload-artifact` 7.0.1 to verified upstream commit references. Updates
require reviewing upstream provenance and permissions. A tag comment is not a
substitute for the immutable reference.

### Phone validator replacement

Reviewed on **2026-09-23 UTC**: the published `phonenumber` 0.3.10 and its
upstream development branch still activate postcard's default heapless dependency.
Postcard 1.1.3 has no published fix for that chain. Darkhorse now pins the maintained
registry release **rlibphonenumber 2.2.12**, published 2026-09-10 from upstream
[`483eae4`](https://github.com/vloldik/rlibphonenumber/tree/483eae4c391cd9508ed73466dfdea836a0f55144).
This replaces the validator in the adapter without a vendored fork or advisory
exception. The new library is younger and less widely adopted. This review and
passing tests do not establish an independent security audit.

Defaults are disabled. Explicit `builtin_metadata`, `global_static`, `regex`,
and `protox` features embed metadata, share the initialized validator and compile
the bundled protobuf definitions with Rust. Installation fetches locked packages.
Builds/tests need no system `protoc`, remote metadata, or runtime files. The
crate's Rust minimum is 1.88, below the pinned toolchain. Parse/validate/format
and build paths were reviewed. Core types, database phone columns and HTTP
error redaction remain project-owned. Inputs reach the parser only after the
domain's ASCII-digit and 15-digit bounds. The library's trace messages contain
phone data. Darkhorse installs no `log` logger or tracing bridge. Any future
logging integration must exclude those dependency messages before activation.

The metadata version moves from 9.0.33 to 9.0.39. A comparison using 4,403 unique
bounded numbers from the old bundled examples plus shortened, appended-zero,
zero-filled and changed-final-digit variants found 63 validation differences.
All 63 agreed with the independently maintained Python `phonenumbers` 9.0.39
reference. This is compatibility sampling, not exhaustive conformance. Committed
source-only regressions cover representative differences, canonical leading
zeros, shared and international service codes, absent fields, invalid prefixes,
extensions, incorrect calling-code splits and fixed HTTP failures. The dropdown
preserves all 207 previous calling codes and adds `800`, `808`, `870`, `878`,
`881`, `882`, `883`, and `888`. Historical contacts remain readable without
applying new numbering rules. New input is always validated before persistence.

The full lockfile removes `atomic-polyfill`, `heapless`, and `postcard`. Rust
package count increases from 327 to 351, largely from protobuf/Unicode build
tooling. The direct replacement and its macro crate are Apache-2.0. Added
packages declare MIT, Apache-2.0, MIT/Unlicense alternatives or Zlib, with optional
LLVM-exception alternatives on rustix/linux-raw-sys. Preserve applicable code
and bundled-data notices. The complete release license inventory and project
license decision remain separate work. No independent dependency audit or
performance improvement is claimed.

The **2026-09-23 UTC** full-lockfile check with cargo-audit 0.22.2 and pnpm
11.19.0 passed for 351 Rust and 336 JavaScript packages, with no vulnerability,
informational or yanked-package findings. The RustSec database revision was
`f7dc4b2860b29978f400fda0aab31cc4dbd21134`. Complete target graphs for `all`,
`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, and
`aarch64-apple-darwin` contain no `atomic-polyfill`. The scanner still checks the
entire lockfile. Rerun the command for current evidence and consult #28 for the
exact committed revision and hosted result. All other release gates remain.
