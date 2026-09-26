# Hosted database and native security tests

The CI workflow runs the complete PostgreSQL and Redis/native suites in two
independent jobs on `ubuntu-24.04`:

| Check                                  | Underlying target    | Boundary                                                                                                                         |
| -------------------------------------- | -------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| PostgreSQL boundary verification       | `make test-postgres` | Migrations, transactions, races, authority, operator persistence and restricted database roles                                   |
| Redis and native security verification | `make test-redis`    | Real Redis/TLS/failure behavior, shared attempt budgets, separate-process enforcement and authenticated native operator commands |

Both run for pushes to `main`, pull requests targeting `main`, and manual dispatch.
The matrix does not cancel one suite because the other fails. These jobs complement
source verification and dependency qualification; they do not replace either.
Isolated unit tests still need no services, runtime settings or external fixtures.

## Execution and resource budgets

Each job checks out source without persisting credentials, installs Node 24.19.0
through the existing pinned action, and fetches locked Cargo dependencies with the
repository's Rust toolchain. Test execution uses locked, offline Cargo commands.
The job does not install frontend packages or start a browser. Docker and OpenSSL
come from the supported hosted runner. The evidence records their actual versions.
Percona PostgreSQL and Redis images are pinned by digest in
[`boundary-images.mjs`](../scripts/lib/boundary-images.mjs).

A job has a 40-minute ceiling. Dependency preparation has 10 minutes, the process
and cleanup fixture has five, the complete-suite step has 27, and fallback cleanup
has two. The total job ceiling applies even if the individual maxima would sum to
more. The suite supervisor itself interrupts work after 25 minutes, leaving time
for cleanup before the outer deadline. Rust compilation uses two jobs with debug
information and incremental compilation disabled. These are CI resource bounds,
not application latency or capacity targets.

The wrapper invokes the ordinary Make target once. It does not filter tests,
retry failures, enable a positive authorization cache or disable security checks.
It bounds combined child stdout/stderr to 2 MiB in memory. Overflow, interruption,
missing commands, nonzero exit or malformed/incomplete summaries fail the check.
Raw output is not published by the wrapper. Reports include bounded failed test
identifiers and counts; reproduce with the underlying target for detailed private
diagnostics. Preparation-step compiler/tool errors remain ordinary runner output.

## Evidence and intentional worker handling

Each job retains only `.local/ci-boundary/<suite>/report.json`, for 14 days, under
an artifact name containing the suite, run ID and attempt. The report includes the
source commit, tracked-change flag, relevant input hashes, image references,
OS/architecture/resource observations, tool versions, UTC timestamps, exit status,
per-binary test counts and verified cleanup outcome. A new run replaces an old
report with `incomplete` before execution, so an interrupted run cannot reuse a
previous pass. These are test summaries, not signed provenance attestations.

PostgreSQL must report one nonempty libtest suite without failures, ignored,
measured or filtered tests. Redis must report two nonempty suites. Its limiter
binary has one deliberate ignored entry: `multiprocess_worker`. The parent test
`separate_processes_share_one_budget_without_shared_connection_pools` invokes that
entry twice as independent processes and verifies their exits and shared budget.
The report accepts this exclusion only when both the specific ignored entry and
the passing parent are present. No other skipped or filtered case qualifies.
Adding another test binary or a legitimate worker requires reviewed parser tests
and an updated contract, not a blanket ignored-test exemption.

A summary is usable only with the corresponding job's final outcome. A passed
suite report from an otherwise failed or cancelled job does not make the job pass.
Coverage, browser/Compose/Kubernetes qualification, independent security review,
fork-policy qualification and production benchmarks remain separate evidence.
The existing 100% authored-code coverage targets are unchanged.

## Cancellation and cleanup

The wrapper assigns a random run identifier to its containers and networks through
`org.darkhorse.boundary-run`. Existing harness `finally` cleanup runs first. The
wrapper and an `always()` fallback step then select only that exact label, remove
owned containers with their anonymous volumes, remove owned networks, and verify
that no matching resources remain. No global prune or name-prefix deletion is
used. Normal development services and concurrent test runs have different owners.
The owner receipt stays local and is excluded from artifact upload.

SIGINT/SIGTERM and the internal deadline stop the owned process group. The real
process fixture verifies failed exits, missing executables, deadlines, output
bounds, interruption and Docker cleanup before running the suite. Cleanup errors
fail rather than becoming a successful report. A forced runner termination or
lost Docker daemon can prevent in-process/fallback cleanup; the disposable hosted
runner's destruction is the final containment boundary. Do not describe the
fallback as guaranteed after host loss. GitHub documents its escalation timings
in the [workflow cancellation reference](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-cancellation).

## Reproduction and maintenance

After installing the documented dependencies and starting Docker:

```sh
make test-ci-boundary-tools
make test-ci-boundary BOUNDARY_SUITE=postgres
make test-ci-boundary BOUNDARY_SUITE=redis
# If an interrupted run left owned resources:
make cleanup-ci-boundary BOUNDARY_SUITE=redis
```

Run at most one wrapper per suite in a checkout; each suite owns one receipt and
report location. Different checkouts and matrix jobs remain independent. Use
`make test-postgres` or `make test-redis` for unfiltered local diagnostics. Do not
publish raw diagnostic output without reviewing it for fixture credentials.
Never supply production database settings to disposable test workflows.

When changing a harness, pinned image, toolchain or runner, repeat pure report and
cleanup tests, real supervision tests and both affected boundary suites. Inspect
the exact-commit hosted summaries, counts, exit status and final job results.
Keep failures visible; investigate rather than retrying until green. The parser
accepts actual test counts, so ordinary added tests do not need hardcoded totals.

The workflow grants only `contents: read`, uses immutable action references, and
contains no repository secrets, publishing steps, dependency caches or
`pull_request_target`. Fork approval policy and branch protection must still be
verified as repository-administration work. A deliberately failing hosted PR and
actual hosted cancellation remain qualification criteria in
[issue #34](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/34)
until their evidence is recorded; local failure fixtures do not substitute for
those hosted observations.
