# Engineering rules

## Boundaries

Dependencies point inward: composition → adapters → application → domain. The core has no HTTP, serialization, environment, SQL, cache, filesystem, clock, or cloud dependencies. Introduce project-owned ports only when an actual use case needs them. Adapters translate their library types into project-owned inputs.

Internal identity references use distinct types for principals, applications, resources, roles, capabilities, scopes, clients, and credentials. They preserve nonzero 128-bit values; an application ID cannot substitute for a client ID at a typed boundary. Adapters own external string formats, serialization, and generation. These identifiers are references, not authentication secrets or proof of access.

Functions are either computations or coordinators. Computations decide and transform explicit inputs without external effects. Coordinators sequence effects and call computations; they do not embed business policy. Keep coordinators small, with transaction ownership visible at the use-case boundary. Mechanical error propagation and transport mapping must not conceal business rules.

The frontend uses Svelte 5 runes, TypeScript strict mode, and static output. No server routes, server-only modules, remote functions, sessions, or credentials belong in the frontend. Generated build-time rendering is allowed; Node is not a production application server.

`make architecture-check` checks every declared dependency kind of the core crates, including renamed and development dependencies, and rejects frontend server/test locations. It is an early guardrail, not a proof of all architectural rules. Rust compilation enforces undeclared imports. Review must also inspect cross-module policy placement, ambient access through the standard library, transaction boundaries, and orchestration size.

## Tests

Start with a meaningful failing test, implement the smallest behavior, then refactor. Verify correct results and explicit failure cases; do not write tests merely to increase a number. Test code mirrors production source under `tests/unit`, with module-level Rust inclusion preserving private APIs. Unit inputs and fakes are defined in source. Unit tests do not read fixtures/settings, mutate process environment, make network calls, or start infrastructure.

Component tests use jsdom and fake API ports. The real browser, TLS, process lifecycle, and static-file checks are separate integration evidence. New behavior needs appropriate tests at its actual boundary.

The target remains 100% coverage of authored executable logic. Coverage is evidence of execution, not proof of correctness. Report pure/unit coverage separately from combined process, browser, and adapter coverage. Show untested entrypoints and coordination paths instead of silently excluding them. Upstream vendored UI components have their own provenance; local changes to them join the authored denominator. Do not claim full coverage until instrumentation and combined suites substantiate it.

The domain authorization suite verifies validated catalogs, live permission computation, and delegation ceilings. It includes exhaustive properties over small capability sets and mutation tests for security checks. These tests do not establish authentication, protocol, persistence, or concurrency correctness; those boundaries require their own suites as they are implemented. See the [authorization contract](authorization.md).

`make coverage-unit` runs two explicit reports. Rust uses cargo-llvm-cov 0.9.1 and `llvm-tools-preview`, with a 100% library line-coverage gate. Install those optional instrumentation tools explicitly with `cargo install cargo-llvm-cov --version 0.9.1 --locked` and `rustup component add llvm-tools-preview`. Stable Rust does not supply branch coverage in this report. The frontend report measures `health.ts`, the connection component, page interaction code, and utility source; upstream button code and static layout/build configuration are excluded. The server executable, tooling entrypoints, and combined process/browser execution are not measured by these unit reports. Full authored-code coverage remains a separate, unfinished qualification requirement.
