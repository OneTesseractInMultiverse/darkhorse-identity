# Engineering rules

Darkhorse's architecture separates policy from effects. Its tests verify correct
results and explicit failures. Contribution records connect each change to an
issue, its evidence, and any incomplete criteria.

## Issue-based contributions

Use [GitHub issues](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues)
to define outcomes and completion criteria. Record decisions, progress, evidence,
and remaining work in the owning issue. One issue can contain several focused
increments. A partial implementation does not close it.

The contributor workflow uses an issue, a short-lived feature branch, and a reviewed
pull request into `main`. External contributors use forks. Maintainers can use
repository branches. Read [Contributing](../CONTRIBUTING.md) for branch names,
validation, review, and merge requirements.

Every commit references its owning issue:

```text
feat: add persistent principal records

Refs #3
```

Use the actual issue number. Fork commits qualify the reference as
`Refs OneTesseractInMultiverse/darkhorse-identity#<number>`. Keep one primary issue
per commit where practical. Use closing keywords only for work that completes all
criteria and required checks. Preserve the issue reference in a squash commit.
Never rewrite published `main` history or force-push shared branches.

Review the staged diff and record actual validation results before committing.
Keep credentials, private planning, and local state outside commits and release
inputs. Public documentation and issues must stand alone. Use project-focused
commit messages without generated attribution or tool branding.

## Dependency boundaries

Dependencies point inward: composition → adapters → application → domain.
The core has no HTTP, serialization, environment, SQL, cache, filesystem, clock,
or cloud dependency. Introduce a project-owned port only for an actual use case.
Adapters translate external types into those project-owned inputs.

Internal identifiers have distinct types for principals, applications, resources,
roles, capabilities, scopes, clients, and credentials. They preserve nonzero 128-bit
values. External formats and identifier generation belong in adapters. Identifiers
are references, not authentication secrets or grants.

Functions are computations or coordinators. Computations decide and transform
explicit inputs without external effects. Coordinators sequence effects and call
computations. Keep coordinators small and transaction ownership visible. Transport
mapping and error propagation must not hide business policy.

The frontend uses Svelte 5 runes, strict TypeScript, and static output. Server routes,
server-only modules, remote functions, sessions, and credentials belong in Rust.
Build-time rendering is permitted. Node is not a production application server.

`make architecture-check` inspects all declared core dependency kinds, including
renamed and development dependencies. It rejects prohibited frontend server and
test locations. This catches structural violations, not every architectural defect.
Review policy placement, standard-library side effects, transaction boundaries,
and coordinator size. See the [architecture guide](architecture.md).

## Interface guidance

Settings explain their purpose, accepted values, defaults and consequences where
users make the decision. Distinguish display names, identifiers and credentials.
Describe access boundaries without implying that a configuration grants authority
or that a saved registration proves an integration works. Connect field guidance
to inputs for assistive technology. Keep tables focused on discovery; use separate
forms and confirmations for changes. Group related controls, disclose optional
detail progressively, and verify keyboard use and narrow-screen layouts.

## Test-driven development

Start a behavior change with a meaningful failing test. Implement the smallest
correct behavior, then refactor. Test successful results, invalid inputs, authority
loss, dependency failures, and relevant concurrency. Avoid tests that merely repeat
the implementation or increase a percentage.

Tests mirror production paths under `tests/unit`. Rust module inclusion preserves
private APIs. Unit inputs and fakes live in source. Unit tests require no external
fixtures, settings, services, process-environment mutation, or network calls.

Component tests use jsdom and fake API ports. Real browser, TLS, terminal, process,
SQL, Redis, and object-storage behavior needs separate integration evidence.
New behavior requires tests at its actual boundary.

## Security changes

State the authority source and transaction boundary before implementation. Identify
which facts need a fresh primary read. Preserve exclusive-writer and shared-reader
ordering. A preflight check never replaces the final mutation check.

Commit security changes and their required audit together. Bound inputs, expensive
work, queues, deadlines, and retained data. Use maintained cryptographic libraries
through adapters. Do not implement cryptographic primitives in project code.

Describe unknown outcomes explicitly. A failed transport or output stream can follow
a committed mutation. Automatic retry requires a proven idempotency contract.
External service calls need defined reconciliation or cleanup behavior.

## Continuing security and performance review

Every increment records its security and performance impact in the owning issue
and pull request. Identify the affected runtime paths and the evidence needed
before implementation. Documentation-only changes can state that they affect no
runtime behavior; other changes explain why existing evidence is sufficient or
which boundary checks and measurements must be repeated.

Review authority and data disclosure across the complete operation: input,
admission, lock waits, authoritative reads, mutations, audit, commit and output.
Include state-dependent errors, proof expiry and intentional self-revocation.
Check input and work bounds, secret exposure, dependency failure and uncertain
outcomes. Newly suspected vulnerabilities follow the
[private reporting policy](../SECURITY.md); public work items contain only suitable
planning and qualification details.

For changes to frequently used paths, queries, locks, indexes, worker concurrency,
caches or dependency behavior, state a performance hypothesis and compare a
reproducible baseline. Record source and dependency versions, topology, population,
arrival pattern and protection settings. Inspect useful throughput, scheduled
latency percentiles, failures and dropped work alongside the relevant CPU, memory,
database, Redis and queue costs. A bounded result set does not prove bounded scan
work. Use query plans and contention evidence where appropriate. Small fixtures
and hosted CI duration do not establish production capacity.

An optimization must preserve coherent primary reads, post-commit revocation,
credential and scope boundaries, shared admission, required audit, and conservative
failure behavior. Do not bypass these controls in the comparison or cache positive
authorization decisions. Enable computation caching only after equivalent-security
measurements demonstrate a useful gain. Keep unknown workload budgets explicit.
The [performance guide](performance.md) defines the available measurement tools
and their limits.

Search the backlog when a review finds a gap. Reuse the owning issue or create a
focused follow-up with evidence, priority, dependencies and observable completion
criteria. Distinguish a confirmed defect from an optimization hypothesis or missing
qualification. Link follow-ups from their parent issues, record what remains, and
repeat the affected review after material schema, workload, dependency or
deployment changes and before release claims. Passing coverage alone clears none
of these requirements.

## Coverage and qualification

The target remains 100% of authored executable logic. Coverage demonstrates
execution, not correctness. Report isolated and combined coverage separately.
Retain uncovered entrypoints and coordination paths in the relevant denominator.
Local changes to upstream UI components join the authored denominator.

`make coverage-unit` reports Rust libraries and frontend unit scope separately.
Database effects leave the Rust unit report below its unchanged 100% line gate.
Combined PostgreSQL and Redis reports add real effects but remain incomplete.
Core-only coverage cannot stand in for whole-system coverage.

[Verification](verification.md) defines targets, instrumentation versions, exclusions,
latest dated evidence, and practical limits. [Release qualification](release-readiness.md)
defines the remaining gates. Report failures and skips directly. Do not lower a
gate or remove executable paths to manufacture completion.
