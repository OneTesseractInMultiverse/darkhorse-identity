# Dependency boundaries

Application lockfiles are committed. Update dependencies in a reviewed change, run the documented checks, and record compatibility/security findings. Dependency installation is separate from offline test execution. No claim of a complete dependency security audit is implied by a successful build.

- **Axum 0.8.9**, Serde, Tokio, and Tower are confined to HTTP/runtime adapters and composition. Transport rejections redact malformed request contents; request body limits remain explicit at consuming routes.
- **envbind 0.1.0** is pinned at the configuration adapter. Tests use `MapEnvironment`; complete settings, including library defaults, are validated before listener binding. Only the composition root reads process configuration. [API documentation](https://docs.rs/envbind/0.1.0/envbind/)
- **restqs 0.1.0** is pinned at the query adapter. The foundation example permits only a status filter and bounded pagination, rejects duplicate/unknown parameters, and maps into `DirectoryCriteria`. It is not a public directory endpoint or an authorization filter. Future SQL adapters must combine caller restrictions with independently established mandatory access predicates. [API documentation](https://docs.rs/restqs/0.1.0/restqs/)
- **SvelteKit**, **Svelte**, **Tailwind CSS**, and **shadcn-svelte** provide the static TypeScript UI. Exact resolved versions are in `pnpm-lock.yaml`. The button and utility source were installed using shadcn-svelte CLI 1.6.1. Styling uses local system fonts, with no remote font requirement.

## Redis preparation

Redis is accepted for the planned shared limiter and versioned computation cache. It is not connected by this foundation. Current candidates reviewed on 2026-09-15 are Redis Open Source **8.10** and redis-rs **1.7.0**; exact server patch/digest, Rust features, dependency advisories, and operating license must be rechecked and locked with the infrastructure slice. [Redis releases](https://redis.io/docs/latest/operate/oss_and_stack/stack-with-enterprise/release-notes/redisce/), [redis-rs API](https://docs.rs/redis/1.7.0/redis/), [Redis licenses](https://redis.io/legal/licenses/)

Use separate cache and limiter instances/configuration, identities, memory policies, deadlines, and recovery behavior. Define secrets and transport settings only in infrastructure configuration; client types never enter core contracts. PostgreSQL remains authoritative. New authorization checks after a revocation commit must observe authoritative state before reusing versioned computation. Cache absence/corruption is a miss; limiter failure requires its own explicit security policy. Unit commands must never provision Redis.
