# Performance baseline

[Issue #10](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/10) tracks performance qualification before authorization computation caching. The initial harness measures the complete HTTPS introspection request against a release Rust server. It also provisions credentials through real browser login, consent, PKCE code exchange and independent ID-token verification.

## Reproduce

Install the [repository toolchain](../README.md#quick-start), Docker, OpenSSL and the pinned browser, then run:

```sh
make deps-install
make browser-install
make benchmark          # smoke: four resource clients, 128 requests per main phase
make benchmark-baseline # baseline: eight resource clients, 2,048 requests per main phase
```

Both commands build the static console and release server with locked dependencies. They create disposable Percona PostgreSQL and separate Redis limiter/cache containers, a temporary TLS certificate, a Rust server and Chromium. They require neither existing local settings nor host certificate trust. The existing development database, credentials and volumes are untouched. Owned processes, containers, anonymous volumes and temporary secrets are cleaned up on exit. The browser fixture advances only disposable recovery/key-publication setup timestamps; runtime authorization and limiting remain enabled.

The client verifies the temporary CA and localhost name. The TLS endpoint is the test harness's Node TLS proxy, with one Rust backend over loopback. This is a single-host measurement, including the host/container database boundary, rather than the planned production reverse-proxy or Kubernetes topology. Database and Redis links are plaintext over isolated local test infrastructure. The PostgreSQL pool is fixed at five connections. The token-route admission limit remains 16 concurrent requests per server process. No positive decisions or authorization computations are cached. Redis shared login limiting is enabled; a shared introspection-attempt limiter is **not implemented**.

Use the same revision, profile, hardware, Docker resource allocation and idle-host conditions for comparisons. Do not run other load tests alongside the benchmark. Repeat runs to assess variability. The runner bounds workers and total attempts; it has no production-target URL option.

## Workloads and interpretation

Every fixture has its own application, confidential OAuth client, resource, introspection credential, `operate` scope and role with read/write capabilities. They share one signed-in principal and SSO session. This small policy graph is a reproducible initial fixture, not a realistic large-organization population.

| Phase                            | Purpose                                                                                                                                                            |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Browser login and SSO            | One valid password login, followed by one authorization-code flow per application using the same session; includes browser automation overhead.                    |
| First pass, new TLS connections  | First measured checks with connection reuse disabled. Provisioning already warmed database state; this is **not** a cold-storage measurement.                      |
| Warmup                           | 64 checks at concurrency eight, reported separately.                                                                                                               |
| Repeated and diverse credentials | Repeated checks of one token versus round-robin clients/tokens at concurrency 1, 8 and 32; the baseline adds 64.                                                   |
| Noisy invalid client             | Six eighths invalid Basic secrets, one eighth valid introspection and one eighth health requests at concurrency 32; outcomes and latency remain grouped by client. |
| Concurrent permission reduction  | Remove a role capability in an actual committed transaction while checks run; retain only the read capability on subsequent checks.                                |
| Concurrent revocation            | Revoke a token through the authenticated HTTPS endpoint while checks run; subsequent checks must return only `active: false`.                                      |
| Explicit post-commit checks      | 64 requests after each change acknowledgement; require useful successful responses so unavailable responses cannot pass the freshness check vacuously.             |
| Unaffected resource              | Confirm another application's token remains usable.                                                                                                                |

Workers send the next request after the previous response completes. This **closed-loop** model measures behavior at a bounded concurrency; slow responses reduce offered traffic. Its percentiles omit time an open-loop arrival would spend waiting before dispatch (coordinated omission). Do not present its throughput as a maximum capacity or an arrival-rate SLO.

Each attempt is timed from immediately before the HTTPS request through response parsing. The reference client has a five-second socket inactivity timeout; the server retains its ten-second route deadline. Classification verifies issuer, audience, subject, issuing client, scope and exact live capabilities. A new request always goes to the server. Inactive results must contain only `active: false`; unexpected successful authorization fails the run. Expected 401 responses and inactive tokens are denials, never counted as authorized throughput. Admission 429/503 responses are separately retained as unavailable. Transport errors, unexpected responses or authority mismatches fail the run. A passing run can still have severe overload rejection; inspect its outcome counts.

For a concurrent change, checks dispatched before the change acknowledgement can legitimately see either state. The runner captures the expected state at dispatch; every check dispatched after acknowledgement requires the reduced/inactive state. Request start times, epochs and change-start/acknowledgement times are recorded. The acknowledgement is a conservative observable boundary after database commit. Inspect timestamps to determine how much load actually overlapped each mutation; the explicit post-commit phases always run. This evidence does not make a consuming application's later business transaction atomic with introspection.

## Reports

Each invocation creates a private `.local/benchmarks/run-*/` directory:

- `report.json`: schema version, completion status, profile, timestamp, commit, dirty-source flag, source/binary SHA-256, Rust/Node/database versions, pinned container images, host/Docker resources, topology, protection settings, SSO samples and per-phase/per-client summaries.
- `requests.jsonl`: one redacted record per attempt with phase, request index, synthetic client index, dispatch epoch, start offset, elapsed milliseconds, HTTP status and classified outcome. No passwords, tokens, Basic headers, signing keys, response bodies or real principal identifiers are saved.

Reports are written after each phase and on exit. An incomplete report must not be interpreted as a passing benchmark. Source hashing includes public application, crate, tooling, configuration and dependency inputs, including untracked implementation files. It excludes private planning and generated output. If the source was dirty, the recorded commit alone cannot reproduce the run; use the source digest and the exact change set. Reports remain ignored by Git; selectively publish reviewed, redacted measurements with an issue update when useful.

Summaries include nearest-rank p50/p95/p99 for all attempts and separately for authorized checks, total and authorized throughput, denial/error fractions and outcome counts. Per-client throughput uses the whole phase duration. Keep error counts beside latency: rapid overload rejections can make the all-attempt p95 look deceptively good.

Before/after observations include server cumulative CPU time/RSS, database-container CPU/memory/block I/O, cumulative PostgreSQL transaction/buffer/tuple counters, and snapshots of activity waits/ungranted locks. These are coarse observations, not a continuously sampled profiler. PostgreSQL statistics can lag; counters include fixture/observer work. Tuple changes and container block I/O are not an exact WAL/write-volume measurement. Boundary lock snapshots cannot exclude contention during the workload.

## Remaining qualification

The harness establishes a repeatable uncached starting point. Issue #10 stays open for larger user/policy populations, genuinely cold database state, open-loop sustained loads, multi-host/production proxy measurements, SQL query plans and round-trip attribution, WAL/write volume, pool-acquisition and lock-wait instrumentation, continuous CPU/memory sampling, and shared-limiter overhead comparisons. SSO samples currently cover one principal, not password-login saturation or a latency distribution.

Latency, capacity and error budgets remain unset until workload goals and repeated measurements support them. Freshness has an explicit security requirement: no stale grant on a check started after the acknowledged commit. Authorization caching must retain the same authentication, admission, failure and freshness semantics when compared with this baseline. The current measurements do not justify enabling it yet.
