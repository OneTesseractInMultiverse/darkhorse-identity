# Darkhorse documentation

These guides describe the implemented server and its operational contracts.
Read [implementation status](implementation-status.md) first. An implemented feature
can still require load testing, protocol conformance, or an independent security review.

## Reading paths

| Reader                | Suggested order                                                                                                                                                                                 |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Application developer | [Provider](provider.md), [registration](registration.md), [identity checks](token-checks.md), [refresh](refresh-tokens.md)                                                                      |
| API developer         | [Authorization](authorization.md), [resource issuance](resource-issuance.md), [introspection](resource-introspection.md), [personal keys](personal-api-keys.md)                                 |
| Deployment operator   | [Architecture](architecture.md), [configuration](configuration.md), [database authority](database-authority.md), [Compose](compose.md) or [Kubernetes](kubernetes.md), [CLI](cli.md)            |
| Contributor           | [Development](development.md), [engineering](engineering.md), [verification](verification.md), [contribution workflow](../CONTRIBUTING.md)                                                      |
| Security reviewer     | [Architecture](architecture.md), [operator authority](operator-authority.md), [dependencies](dependencies.md), [release requirements](release-readiness.md), [reporting policy](../SECURITY.md) |

## Identity and access

- [Password authentication](authentication.md) defines session cookies, hashing, and shared attempt limits.
- [Authorization](authorization.md) defines roles, capabilities, scopes, and immutable credential ceilings.
- [API reference source inventory](api-reference.md) explains the generated Axum registration snapshot and its strict limits; the integrated OpenAPI reference is tracked in issue #43.
- [Client registration](registration.md) defines applications, callbacks, client secrets, and resource allowances.
- [Signing reconciliation](signing-operations.md) explains operation receipts and recovery after lost replies.
- [OIDC provider](provider.md) covers signing keys, consent, authorization codes, and ID tokens.
- [Resource issuance](resource-issuance.md) binds user authority to a single protected resource.
- [Identity checks](token-checks.md) covers UserInfo, client introspection, and revocation.
- [Resource introspection](resource-introspection.md) defines API credentials and live capability checks.
- [Refresh tokens](refresh-tokens.md) defines rotation, family replay, and cleanup.
- [Session management](sessions.md) covers owner-visible history and individual termination.
- [Back-channel logout](backchannel-logout.md) separates implemented session references from planned delivery.
- [Personal API keys](personal-api-keys.md) defines delegation, expiration, and resource checks.

## Accounts and administration

- [Email verification](email-verification.md) proves control of the current address.
- [Invitations](invitations.md) creates ordinary accounts through purpose-bound links.
- [Localization](localization.md) describes English/Spanish presentation, fallback, typed catalogs and current limits.
- [Translation contributions](localization-contributing.md) defines catalog workflow, style, security meaning, provenance and pseudolocalization.
- [Localization release checklist](localization-release.md) separates web, OIDC, email and CLI evidence from human release review.
- [Console](console.md) describes user directory operations and browser failure handling.
- [Catalog administration](catalog-administration.md) manages explicit application and permission bindings.
- [Profiles and media](profiles-and-media.md) covers descriptive fields, private images, and login branding.
- [CLI](cli.md) specifies parsing, protected input, output, confirmation, and process behavior.
- [Operator authority](operator-authority.md) identifies the authority required by each command.
- [Operator accounts](operator-accounts.md) defines per-command administrator authentication.
- [Operator access catalogs](operator-access-catalog.md) lists resources, scopes and explicitly selected permission definitions.
- [Operator catalog reads](operator-catalog.md) specifies bounded application and client inspection.
- [Operator application writes](operator-applications.md) defines complete specifications, revision checks and transactional audit.
- [Client-secret inventory and retirement](operator-client-secrets.md) defines bounded lifecycle metadata, scoped retirement and audit reconciliation.
- [Operator client updates](operator-clients.md) defines complete configuration input, current authority and atomic audit.
- [Container account launchers](container-accounts.md) documents protected stdin and remote outcome uncertainty.

## Storage, deployment, and recovery

- [Migration reconciliation](migration-operations.md) explains partial batches, receipts, and interrupted upgrades.
- [Persistence](persistence.md) covers bootstrap, migrations, account epochs, and SQL connections.
- [Database authority](database-authority.md) defines runtime, operator, and migration grants.
- [Percona containers](percona.md) documents the PostgreSQL distribution and volume transition.
- [Redis](redis.md) explains atomic attempt charging, continuity checks, and recovery waits.
- [Limiter activation](limiter-activation.md) documents durable intent, receipts, and read-only inspection.
- [Compose](compose.md) provides an isolated packaged deployment and backup procedure.
- [Kubernetes](kubernetes.md) defines replicated runtime and explicit operator workloads.
- [Configuration](configuration.md) maps feature dependencies, secret delivery, and persistent bindings.

## Engineering evidence

- [Architecture](architecture.md) explains code dependencies and transaction boundaries.
- [Development](development.md) provides the local workflow.
- [Verification](verification.md) maps tests to the claims they support.
- [Hosted boundary tests](hosted-boundary-tests.md) explains database/native CI jobs, budgets, reports and cleanup.
- [Performance](performance.md) records measurement methods and dated results.
- [Dependencies](dependencies.md) records library choices and advisory evidence.
- [Engineering rules](engineering.md) defines implementation and review requirements.
- [Release qualification](release-readiness.md) lists unresolved production gates.
- [Third-party notices](third-party-notices.md) preserves upstream licensing information.
- [Brand assets](brand/README.md) describes the checked-in visual assets.

## Reading diagrams and contracts

A diagram summarizes one contract. Its caption states the relevant scope.
Flowchart arrows describe relationships. Dashed flowchart arrows mark optional,
reserved, or planned relationships according to the caption. Sequence diagrams use
solid calls and dashed replies by default. The logout diagram labels its planned
delivery section explicitly. State diagrams show legal transitions. They do not replace the
input limits, failure rules, or transaction details in the surrounding text.

Commands assume the repository root. Values inside angle brackets are placeholders.
Configuration names and protocol fields retain their exact spelling. Never place
credentials in command arguments or tracked files. Use the protected inputs named
by each runbook.
