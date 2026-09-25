# Implementation status

This inventory describes the current source, reviewed on **2026-09-24**. It includes
the operator journals, authenticated directory/catalog reads and application writes under
[#23](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/23),
[#25](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/25) and
[#26](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/26), plus
account/catalog container launchers under
[#27](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/27).
Darkhorse has no qualified production release and no selected project license.

## Status vocabulary

**Implemented** means the repository contains the behavior and relevant tests.
**Partial** means a named part works, but the broader feature remains incomplete.
**Planned** means the public contract or issue exists without a supported implementation.
A passing test demonstrates its exercised boundary. It does not establish complete
conformance, fault tolerance, capacity, or security.

## Feature inventory

| Area                      | State       | Implemented boundary                                                                                                                        | Remaining work                                                                                   |
| ------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Deployment model          | Implemented | One organization per deployment                                                                                                             | Shared multi-organization hosting has no isolation model                                         |
| Architecture              | Implemented | Rust domain/application/adapters, static SvelteKit TypeScript console                                                                       | Continued boundary review and full authored-code coverage                                        |
| Password authentication   | Implemented | Argon2id, opaque sessions, shared Redis attempt budgets                                                                                     | Password screening, privileged MFA or step-up, recovery assurance                                |
| OIDC code flow            | Implemented | Confidential clients, Basic authentication, S256 PKCE, discovery, JWKS, RS256 ID tokens                                                     | Full provider conformance and broader client interoperability                                    |
| Resource authorization    | Implemented | Explicit role assignments, capabilities, scope bounds, immutable ceilings                                                                   | Delegated administration and production scale qualification                                      |
| Access tokens             | Implemented | Opaque identity and single-resource tokens, five-minute maximum lifetime                                                                    | Further fault, retention, and load qualification                                                 |
| Refresh tokens            | Implemented | Explicit client opt-in, rotation, family replay revocation, bounded cleanup                                                                 | Offline access, public clients, and remaining operational qualification                          |
| Token checks              | Implemented | Scoped UserInfo, client introspection, dedicated resource introspection, revocation                                                         | Distributed token-route abuse controls and sustained-load qualification                          |
| Personal API keys         | Implemented | User-owned application keys, selected resource ceilings, optional expiry                                                                    | Independent service identities, usage summaries, retention policy UI                             |
| Session management        | Implemented | Owner history and individual termination, local logout, epoch revocation                                                                    | Broader administrator session commands and retention                                             |
| Back-channel logout       | Partial     | Persisted relying-party references and signed `sid` ID-token claim                                                                          | Destination registration, Logout Tokens, outbox, delivery, receiver qualification                |
| Email verification        | Implemented | Current-address proof and authenticated implicit-TLS SMTP delivery                                                                          | Email change, key/origin migration, operational retention                                        |
| Invitations               | Implemented | Administrator-issued links for new ordinary accounts                                                                                        | Broader onboarding policy and delivery qualification                                             |
| Password recovery         | Planned     | No supported reset or change workflow                                                                                                       | Recovery policy, proof lifecycle, notifications, assurance                                       |
| Console                   | Implemented | User directory, application/client/resource/scope/role/capability pages                                                                     | Audit browser, delegated administration, full accessibility review                               |
| Profiles and media        | Implemented | Extended fields, private S3 images, public login branding                                                                                   | Extra OIDC claims, storage migration, fuzzing and restore qualification                          |
| CLI                       | Partial     | Bootstrap and operation journals; authenticated accounts, catalog reads, application writes, client updates and secret inventory/retirement | Scoped emergency authority, client creation/secret delivery and broader account/catalog commands |
| Database roles            | Implemented | Independent runtime, nonowner operator, and schema-owner credentials                                                                        | Stronger compromise containment and exceptional-credential lifecycle                             |
| Limiter journal           | Implemented | Intent before Redis, atomic PostgreSQL completion, read-only inspection                                                                     | Protected evidence export/retention and broader recovery reconciliation                          |
| Redis authorization cache | Planned     | Separate cache deployment exists                                                                                                            | Versioned computation caching with fresh primary checks                                          |
| Compose                   | Implemented | Packaged HTTPS, private services, reviewed grants, backup and isolated recovery tests                                                       | Production topology, certificate lifecycle, coordinated disaster recovery                        |
| Kubernetes                | Implemented | Explicit namespace/context, two-replica tests, network policies, operator/migrator Jobs, account/catalog launchers and one-shot Pods        | Multi-host faults, production CNI/storage qualification, operational readiness                   |
| Passkeys                  | Planned     | No WebAuthn authentication or authenticator lifecycle                                                                                       | [Issue #22](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/22)            |

## Important distinctions

The browser session cookie contains a random handle. It carries no encrypted claim
payload. Access and refresh tokens are opaque. ID tokens are signed JWTs.
Signed Logout Tokens belong to the planned delivery implementation.

A personal API key represents a user. It does not create a separate service principal.
Application ownership records a contact. It grants neither platform administration
nor resource access. A scope restricts delegated capabilities. It grants no role membership.

The private profile API includes country, phone, secondary names, bio, and pictures.
UserInfo exposes only the currently supported, consented claim set. New profile
attributes do not expand previously issued tokens.

Runtime credentials have restricted grants, but they retain broad application DML.
Operator credentials remain powerful deployment credentials. The CLI's account
authentication adds a per-command check. It does not contain arbitrary direct SQL
from a stolen database credential. Read [database authority](database-authority.md).

## Schema inventory

The binary embeds **29 forward migrations** in
[`crates/adapters/migrations`](../crates/adapters/migrations).
HTTP startup does not apply them. Existing installations require explicit migration
and grant review with writers stopped.

| Versions      | Introduced state                                                                      |
| ------------- | ------------------------------------------------------------------------------------- |
| `0001`–`0004` | Directory, administrator eligibility, limiter authority, browser sessions             |
| `0005`–`0008` | Registration, signing keys, authorization requests, code exchange                     |
| `0009`–`0012` | Identity checks, resource authority, introspection credentials, refresh families      |
| `0013`–`0016` | Session management, email verification, invitations, relying-party session references |
| `0017`–`0019` | User directory audit, catalog administration, personal keys                           |
| `0020`–`0021` | Extended profiles and media assets                                                    |
| `0022`–`0024` | Account-command audit, limiter activation journal, signing operation journal          |
| `0025`        | Authenticated operator directory-read audit                                           |
| `0026`        | Authenticated operator catalog-read audit                                             |
| `0027`        | Authenticated operator application/client detail-read audit                           |
| `0028`        | Authenticated operator application-write audit                                        |
| `0029`        | Authenticated operator client-configuration audit                                     |
| `0030`        | Client-secret lifecycle audit and paged inventory index                               |

A migration's existence does not certify an upgrade for every data volume or
mixed-version deployment. Each operational guide states its transition requirements.

## Qualification and issue tracking

[Verification](verification.md) records the latest evidence and its scope.
[Release qualification](release-readiness.md) defines the unresolved release gates.
Feature issues remain open for outstanding criteria after functional increments land.

The main cross-cutting work remains in
[#1](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/1),
[#2](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/2),
[#21](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/21), and
[#23](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/23).

## Migration operation evidence

The CLI records durable batch intent, an immutable target manifest, atomic per-step
history and receipts, and final completion. Read-only owner inspection distinguishes
historical evidence from current checksum matches. Empty databases and existing
installations use the same workflow. A partially applied batch requires explicit
reconciliation. See [migration operations](migration-operations.md). This increment
leaves emergency authority, production privileged assurance, and broader audit
retention/export work in #23 open.
