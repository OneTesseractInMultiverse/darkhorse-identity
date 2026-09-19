# Performance baseline

[Issue #10](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/10) tracks performance qualification before authorization computation caching. The initial harness measures the complete HTTPS introspection request against a release Rust server. It also provisions credentials through real browser login, consent, PKCE code exchange and independent ID-token verification.

## Reproduce

Install the [repository toolchain](../README.md#quick-start), Docker, OpenSSL and the pinned browser, then run:

```sh
make deps-install
make browser-install
make benchmark          # smoke: four resource clients, 128 requests per main phase
make benchmark-baseline # baseline: eight resource clients, 2,048 requests per main phase
make benchmark-arrivals # paced traffic: four clients, two seconds per arrival phase
make benchmark-arrivals-baseline # eight clients, ten seconds per arrival phase
```

These commands build the static console and release server with locked dependencies. They create disposable Percona PostgreSQL and separate Redis limiter/cache containers, a temporary TLS certificate, a Rust server and Chromium. They require neither existing local settings nor host certificate trust. The existing development database, credentials and volumes are untouched. Owned processes, containers, anonymous volumes and temporary secrets are cleaned up on exit. The browser fixture advances only disposable recovery/key-publication setup timestamps; runtime authorization and limiting remain enabled.

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

`make benchmark` and `make benchmark-baseline` use workers that send the next request after the previous response completes. This **closed-loop** model measures behavior at a bounded concurrency; slow responses reduce offered traffic. Its percentiles omit time an open-loop arrival would spend waiting before dispatch (coordinated omission). Do not present its throughput as a maximum capacity or an arrival-rate SLO.

### Fixed-arrival workloads

`make benchmark-arrivals` and `make benchmark-arrivals-baseline` add an **open-loop** schedule: the next arrival is determined by its planned time, independently of whether earlier responses have finished. After a 64-request warmup, the smoke profile offers 200 and 1,200 arrivals/s for two seconds each; the baseline offers 200, 800 and 1,600 arrivals/s for ten seconds each. Both run the same invalid-client/healthy-client/health-route mixture at 1,200 arrivals/s. Permission reduction and token revocation each run during a separate 200-arrivals/s phase, followed by explicit post-commit checks and the unaffected-resource check. Rates are test inputs, not declared capacity or SLOs.

The schedule is deterministic and evenly spaced. It is anchored to a monotonic clock rather than accumulating timer delays. A single scheduler limits active requests to 128, matching the HTTP agent socket bound. Every planned arrival gets one record:

- Arrivals observed within five milliseconds of their planned time are dispatched, retaining both actual dispatch delay and response latency.
- A late timer, or an arrival observed after the phase's planned end, produces `generator_late`. The request is not sent.
- A full driver concurrency budget produces `generator_full`. The request is not queued or retried.

The five-millisecond tolerance is a driver setting, not an application latency budget. Small catch-up groups within that tolerance are possible; larger backlogs are recorded and discarded. No unbounded pending queue is created. The scheduler waits through the complete planned window and drains outstanding requests on completion or failure. Configuration computations additionally bound each phase to 30 seconds, 2,000 arrivals/s, 30,000 scheduled arrivals and 128 active requests; published profiles use tighter limits.

Security changes begin near the middle of the paced phase. Reports retain their actual start and commit-acknowledgement times; expectations are captured at actual dispatch, not at the earlier scheduled time. Thus a delayed request dispatched after a committed reduction must observe the reduced state. HTTP credentials, primary-state checks, admission and error semantics match the closed-loop workloads.

Scheduled latency runs from planned arrival through response parsing, so it includes driver delay for dispatched requests. Unsent arrivals have no invented HTTP latency; their drop counts remain alongside all percentiles and failure fractions. Driver drops mean the server did not receive the complete offered schedule. A passing correctness run with drops or server rejections does not establish that the configured arrival rate is sustainable. The driver, TLS proxy and server share the host; this is still not a production capacity measurement. Longer endurance tests, varied arrival distributions and a separate load-generator host remain qualification work.

Each attempt is timed from immediately before the HTTPS request through response parsing. The reference client has a five-second socket inactivity timeout; the server retains its ten-second route deadline. Classification verifies issuer, audience, subject, issuing client, scope and exact live capabilities. A new request always goes to the server. Inactive results must contain only `active: false`; unexpected successful authorization fails the run. Expected 401 responses and inactive tokens are denials, never counted as authorized throughput. Admission 429/503 responses are separately retained as unavailable. Transport errors, unexpected responses or authority mismatches fail the run. A passing run can still have severe overload rejection; inspect its outcome counts.

For a concurrent change, checks dispatched before the change acknowledgement can legitimately see either state. The runner captures the expected state at dispatch; every check dispatched after acknowledgement requires the reduced/inactive state. Request start times, epochs and change-start/acknowledgement times are recorded. The acknowledgement is a conservative observable boundary after database commit. Inspect timestamps to determine how much load actually overlapped each mutation; the explicit post-commit phases always run. This evidence does not make a consuming application's later business transaction atomic with introspection.

## Reports

Each invocation creates a private `.local/benchmarks/run-*/` directory:

- `report.json`: schema version (currently 2), completion status, profile, timestamp, commit, dirty-source flag, source/binary SHA-256, Rust/Node/database versions, pinned container images, host/Docker resources, topology, protection settings, SSO samples and per-phase/per-client summaries.
- `requests.jsonl`: one redacted record per dispatched request or planned paced arrival with phase, request index, synthetic client index, dispatch epoch, start offset, elapsed milliseconds, HTTP status and classified outcome. Paced rows also contain planned arrival time, dispatch delay and scheduled latency. Unsent rows have null HTTP status/start/latency, the observation time and a driver-drop outcome. No passwords, tokens, Basic headers, signing keys, response bodies or real principal identifiers are saved.

Reports are written after each phase and on exit. An incomplete report must not be interpreted as a passing benchmark. Source hashing includes public application, crate, tooling, configuration and dependency inputs, including untracked implementation files. It excludes private planning and generated output. If the source was dirty, the recorded commit alone cannot reproduce the run; use the source digest and the exact change set. Reports remain ignored by Git; selectively publish reviewed, redacted measurements with an issue update when useful.

Summaries include nearest-rank p50/p95/p99 for all attempts and separately for authorized checks, total and authorized throughput, denial/error fractions and outcome counts. Per-client throughput uses the whole phase duration. Keep error counts beside latency: rapid overload rejections can make the all-attempt p95 look deceptively good.

Fixed-arrival summaries additionally include configured and scheduled rates, planned duration, peak in-flight requests, driver drops, dispatch-delay percentiles, and scheduled-latency percentiles for dispatched/authorized checks. `attempts` counts only dispatched requests; `scheduled` includes driver drops. Throughput uses the full phase duration including response draining. `scheduledFailureRate` includes driver drops, unavailability, unexpected errors and authority violations over all planned arrivals; expected security denials remain separate. Per-client scheduled rates use that client's own planned arrivals and the same phase duration. No latency percentile includes an unsent arrival.

Before/after observations include server cumulative CPU time/RSS, database-container CPU/memory/block I/O, cumulative PostgreSQL transaction/buffer/tuple counters, and snapshots of activity waits/ungranted locks. These are coarse observations, not a continuously sampled profiler. PostgreSQL statistics can lag; counters include fixture/observer work. Tuple changes and container block I/O are not an exact WAL/write-volume measurement. Boundary lock snapshots cannot exclude contention during the workload.

## Remaining qualification

The harness establishes a repeatable uncached starting point. Issue #10 stays open for larger user/policy populations, genuinely cold database state, longer endurance and production arrival distributions, multi-host/production proxy measurements, SQL query plans and round-trip attribution, WAL/write volume, pool-acquisition and lock-wait instrumentation, continuous CPU/memory sampling, and shared-limiter overhead comparisons. SSO samples currently cover one principal, not password-login saturation or a latency distribution.

Latency, capacity and error budgets remain unset until workload goals and repeated measurements support them. Freshness has an explicit security requirement: no stale grant on a check started after the acknowledged commit. Authorization caching must retain the same authentication, admission, failure and freshness semantics when compared with this baseline. The current measurements do not justify enabling it yet.
