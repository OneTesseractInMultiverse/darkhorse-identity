# Redis infrastructure and admission contracts

The first part of [issue #4](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/4) provides independent cache/limiter services, bounded Rust connection pools, diagnostics, and pure attempt-budget computations. **Live shared admission enforcement is not configured.** No login or authorization path uses Redis yet. Durable recovery and an atomic counter adapter must be completed before password login can rely on it. Cached authorization computations belong to the later cache slice.

## Local services

Start Docker, install the locked project dependencies, then run:

```sh
make redis-setup
make redis-up
make redis-status
make redis-down
```

Setup creates `.local/redis-cache.acl`, `.local/redis-limiter.acl`, and `.local/redis.env` with owner-only permissions. Four independent random credentials cover the two diagnostic users and the two operator users. Existing files are preserved; missing files in a partial setup, symlinks, or broad file permissions cause an error. Keep credentials with the corresponding limiter data; do not regenerate one while retaining the other. Private files are excluded from Git, image contexts and exports.

Each service has its own Docker network, memory allocation and credentials. Host ports bind only to loopback: cache `63791`, limiter `63792`. Different workspaces own different Compose projects, but the default host ports still conflict. To change ports, edit both the port settings and all corresponding URLs in `.local/redis.env`. Services use separate bridge networks so the host-run Rust process can access the loopback publications; these are development networks, not production network-policy qualification.

| Role    | Redis memory limit | Container memory limit | Eviction      | Storage                                         |
| ------- | ------------------ | ---------------------- | ------------- | ----------------------------------------------- |
| Cache   | 64 MiB             | 128 MiB                | `allkeys-lru` | Temporary memory-backed data directory          |
| Limiter | 64 MiB             | 128 MiB                | `noeviction`  | Dedicated volume; AOF with `appendfsync always` |

`redis-down` preserves limiter data and credentials. Cache data is disposable. Neither ordinary unit tests nor HTTP startup provision/connect to these services. Compose health checks verify the local listener requires authentication; `redis-status` separately verifies configured credentials, server role and memory policy. It reports `shared_enforcement: not_configured` even when both connections are reachable.

The diagnostic users can only authenticate, ping, inspect INFO, and set client metadata. They cannot read/write keys, change ACLs or change server configuration. Operator credentials remain in the protected local settings file and are removed from the diagnostic process environment and are not read by application configuration. Counter/cache permissions will be added with their actual adapters. The container entrypoint copies the mounted ACL into a private runtime file, then the official image entrypoint runs Redis as its unprivileged user.

## Rust connection boundary

The adapter uses Redis Open Source **8.10.1**, pinned by image digest, and **redis-rs 1.7.0** with Tokio/rustls and web PKI roots. Automatic connection-manager retries, cluster, sentinel and default client features are disabled. Update the server digest/client in a reviewed change and repeat the failure tests. [Server release notes](https://redis.io/docs/latest/operate/oss_and_stack/stack-with-enterprise/release-notes/redisce/redisos-8.10-release-notes/), [client documentation](https://docs.rs/redis/1.7.0/redis/)

Configuration uses envbind with explicit input validation. Connection settings deliberately lack Debug output. Errors and diagnostics never print URLs, supplied passwords or ACL verifiers.

| Variable                              | Default / requirement                                                    |
| ------------------------------------- | ------------------------------------------------------------------------ |
| `DARKHORSE_REDIS_CACHE_URL`           | Required authenticated URL for cache                                     |
| `DARKHORSE_REDIS_LIMITER_URL`         | Required authenticated URL for limiter                                   |
| `DARKHORSE_REDIS_CACHE_CONNECTIONS`   | 2, range 1–16                                                            |
| `DARKHORSE_REDIS_LIMITER_CONNECTIONS` | 4, range 1–16                                                            |
| `DARKHORSE_REDIS_TIMEOUT_MS`          | 250 ms, range 10–1000 ms                                                 |
| `DARKHORSE_REDIS_INSECURE`            | false; local setup explicitly enables plaintext for loopback development |

URLs are limited to 4096 bytes, must use a named ACL user and explicit password, and must select database zero. Query strings/fragments, default-user authentication, and encoded socket hosts are rejected. TLS uses `rediss://` with certificate/hostname verification; there is no insecure TLS option. Private CA/client-certificate provisioning and positive TLS deployment qualification remain open. When the development plaintext switch is enabled, a `rediss://` connection still requires TLS and never downgrades.

The adapter rejects matching endpoints and equivalent decoded passwords. Diagnostics additionally distinguish server process identities and require standalone primary roles with nonzero memory limits and the expected eviction policy. A limiter already above its memory limit is reported unsafe. These checks are a configuration observation, not a guarantee that the next operation has capacity or that acknowledged counters survived a failure.

Cache and limiter have independent lazy connection pools and operation deadlines. Each pool rejects work immediately when all its slots are occupied; it has no unbounded waiting queue. Connection establishment and all diagnostic commands share one deadline. Failed/timed-out connections are discarded, and a later read-only diagnostic may reconnect. This reconnect behavior does not authorize retrying a counter mutation. Size aggregate connections across application replicas against each server's client budget. These initial limits are not throughput benchmarks.

## Pure attempt policy

`domain::limiting` receives explicit time and counter snapshots; it does not read clocks, call Redis, or mutate external state. The policy accepts one to four distinct budgets, each with a limit of 1–1,000,000 admitted attempts and a window of 1–900 seconds. A fixed window starts at the first admitted attempt. At its exact end, a new window can start. Every requested budget charges together, or none does when a budget is exhausted. A denial reports the longest remaining exhausted window.

Stable budget keys are 32-byte keyed digests selected by the trusted caller. They must never be taken directly from user input; deriving and rotating them belongs with the actual login policy. Thresholds are also server-owned. Raw emails, addresses and tokens must not become Redis keys or metric labels. This counts admitted attempts; later edge/local admission controls must separately bound rejected traffic and expensive work before shared enforcement.

Stored-policy mismatches, corrupt counters and clock rollback fail closed. Time is bounded to integers exactly representable by the intended wire format, and windows cannot overflow that bound. The future adapter must choose and validate authoritative time across replicas; caller wall clocks must not reset global budgets.

The application-owned `AttemptLimiter` port requires an atomic charge against authoritative enforcement state. Its coordinator calls the port once: only a known allowed result permits one immediate attempt. Limited or unavailable results reject it. An uncertain transport result is never transparently retried. A separately submitted attempt may be charged again; conservative overcounting is preferable to an unaccounted allowance. A result is not a reusable session, token or idempotency key.

## Recovery work still required

Redis reconnect, a persistent AOF, a matching process ID, or a successful health check cannot establish that acknowledged counters survived promotion/data loss. Redis replication is asynchronous, and a promoted process can keep its process ID while its replication history changes. [Replication guarantees](https://redis.io/docs/latest/operate/oss_and_stack/management/replication/), [connection and process identity](https://redis.io/docs/latest/develop/programmability/eval-intro/)

Before enabling live enforcement, issue #4 must deliver and test:

- Atomic application of multi-budget proposals, bounded counter cardinality/expiry and policy transitions, without resetting missing state blindly.
- A durable enforcement generation, old-primary fencing, restrictive cooldown/recovery commands, and explicit authority for restoring service after state loss. New application processes must not create a fresh global allowance.
- Ambiguous writes, retries, script reload, simultaneous Rust replicas, restart, partitions, promotion and lost acknowledged counters at the real enforcement boundary.
- Positive TLS/private-CA qualification, operational telemetry, and measured latency/capacity for the supported topology.

Automatic failover is not supported by the current implementation. There is intentionally no operator command that resets counters or declares recovery complete yet. PostgreSQL remains authoritative for durable identity/security state, and future cached authorization computations must preserve strict post-commit revocation.

## Verification

```sh
make test-limiting
make test-redis
make coverage-core
make test-mutation-limiting
make docker-build docker-redis-smoke
```

Unit tests are in source, use explicit time/fake ports, and need no services or settings. The Redis integration runner creates its own random credentials, names, networks and host ports, then removes only those resources. It verifies role identity across separate Rust clients, restricted runtime credentials, independent authentication/ACL/configuration failures, real cache eviction, limiter memory pressure, paused connections, restart identity and TLS downgrade rejection. It also checks redacted CLI output. These are infrastructure tests, not shared-counter or recovery qualification.

`docker-redis-smoke` runs the built application image as its unprivileged user with a read-only filesystem and dropped capabilities, connected to both disposable Redis networks. Only the runtime credentials enter the application container. This checks the packaged diagnostics independently of host Rust tooling.

`coverage-core` measures only the framework-free domain/application crates. It does not replace the unchanged full Rust library coverage gate or imply whole-system coverage. `coverage-integration` combines Rust unit tests with both PostgreSQL and Redis integration/CLI execution and keeps uncovered paths visible. Overall coverage qualification remains open in issues #2/#3; P02 remains open in #4. No performance improvement or production-readiness claim is made yet.
