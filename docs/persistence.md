# Persistence and operator bootstrap

PostgreSQL stores authoritative accounts, sessions, policy, credentials, and audit
records. Versioned migrations define their constraints. This guide covers storage,
bootstrap, lifecycle epochs, and connection behavior. CLI account operations require
[per-command authentication](operator-accounts.md). Bootstrap has no HTTP endpoint.

## Local database

Install Docker with Compose, start its engine, then run:

```sh
make db-setup
make db-up
make db-migrate
make bootstrap
```

Setup creates a random database password and connection settings in `.local/database-password` and `.local/database.env`, with owner-only file permissions. Existing credentials are preserved. Incomplete or broadly readable files are rejected. Keep these files private. They are excluded from Git, image build contexts, and release exports. Do not delete credentials for a retained database volume.

Percona Server for PostgreSQL 18.6.1 (PostgreSQL 18.6 compatible) listens only on `127.0.0.1:54329`. Compose project names are derived from the workspace path, so separate workspaces own separate volumes. Their default host port still conflicts if both run simultaneously. To change the port, update both the port and connection URL in the local settings file. `make db-down` removes this workspace's containers/network and preserves its volume and credentials. `make clean` preserves them. Reopening the workspace at another path creates a different Compose project. The pinned container and existing-volume migration procedure are documented in [Percona containers](percona.md). Existing upstream volumes require logical restoration into the separate Percona volume before cutover.

Migrations are versioned SQL embedded in the Rust binary. Apply them explicitly with `migrate`. Ordinary HTTP startup does not migrate or bootstrap. Active authentication/provider services connect to PostgreSQL. SQLx records migration checksums. The [migration journal](migration-operations.md) records durable intent, per-step receipts, and final completion. Inspection distinguishes historical completion from current matching history. Once published, change the schema with a new migration, not by editing an applied migration. Back up and verify restore procedures before migrating valuable data. No automatic down-migration or destructive reset command is provided.

## Bootstrap input and password storage

Interactive bootstrap requests email, first name, last name, and a hidden password with confirmation. Nothing is supplied as a command argument. It requires a foreground terminal. The [CLI input contract](cli.md#input-boundaries-and-remaining-qualification) defines fixed input bounds, supported editing and cancellation, and terminal restoration. Automated operators can instead pipe protected JSON to:

```sh
./target/release/darkhorse-server operator bootstrap --stdin --yes
```

Supply exactly `email`, `first_name`, `last_name`, and `password` as JSON strings. Input is limited to 16 KiB and unknown fields are rejected. Use a secret manager or protected input stream. Shell command literals and trace logs can retain secrets. The process needs the database settings described below. An explicit password in the database URL prevents implicit password-file lookup. Errors do not include supplied values, SQL parameters, password verifiers, or connection strings. Success reports only the new principal identifier.

Initial validation policy:

- Email: strip surrounding whitespace, accept ASCII dot-atom local parts and DNS-style domains containing a dot, and bound the whole address to 254 bytes, local part to 64, and domain labels to 63. Preserve display spelling. Compare the entire ASCII address case-insensitively using a unique generated database key. Quoted addresses and internationalized addresses are not accepted. The address is not considered verified from syntax validation alone.
- Names: first and last names are required, trimmed, control-free Unicode strings of 1–100 characters each. Extended attributes and image references follow the [profile contract](profiles-and-media.md).
- Passwords: 15–128 Unicode characters, no control characters. Passwords are neither trimmed nor normalized. Password screening and recovery rules remain open security work.
- Password verifiers: RustCrypto Argon2id v19, 64 MiB, three iterations, one lane, 32-byte output and an independent 16-byte operating-system random salt. The PHC string retains its algorithm parameters. Each operator process admits one hashing worker before scheduling. Account
  authentication shares the deployment login budget. Initial bootstrap has its own
  one-time deployment-authority boundary. Plaintext working buffers are zeroized where owned, without claiming that every runtime/OS copy can be erased.

The operator prepares identifiers and the verifier before beginning the database transaction. The transaction locks the singleton security state and inserts the principal, credential, administrator membership, audit record, and consumed bootstrap flag together. Concurrent attempts have exactly one winner. Failed transactions leave none of those writes committed. Bootstrap cannot be reset through the supported interface. If a connection fails during commit, inspect database state before retrying: the outcome may be unknown to the client.

## Authority, lifecycle and revisions

Commands require database access appropriate to their operation. Account commands
require a fresh, verified administrator password and a transaction-time authority
check. Bootstrap, migration, signing, and limiter commands retain deployment-authority
boundaries. Platform administrator membership is separate from application ownership
and grants no bypass of the resource authorization evaluator.

After building the binary, use `operator account show ID` to read the current record. `deactivate ID REVISION`, `reactivate ID REVISION`, and `revoke-all ID REVISION` require its current revision. Supply the same login/limiter/database configuration as the running deployment and authenticate for each command as described in [the account CLI contract](operator-accounts.md). The database-only local wrapper does not supply all of these settings.

Conflicting revisions fail. Read current state before deciding whether to retry. A no-op status change preserves the target revision and existing security audit, but adds an operator outcome record. Real transitions and their audit event commit atomically:

| Change     | Principal revision | Credential epoch |
| ---------- | ------------------ | ---------------- |
| Deactivate | Advance            | Advance          |
| Reactivate | Advance            | Preserve         |
| Revoke all | Advance            | Advance          |

Identifiers are nonzero random UUIDs generated by the adapter. Principal and credential identities cannot change or be hard-deleted through ordinary DML. Capabilities retain immutable IDs, permission keys and meanings. Retirement is one-way. Credential records separate their kind from password material so later credential types can be added independently.

The last **eligible** platform administrator cannot be deactivated, including concurrent attempts. Eligibility currently means active membership with at least one nonrevoked password credential and its verifier record. Membership without a credential does not count. Deferred database constraints reject removal/revocation of the final eligible credential and prevent consuming bootstrap without an eligible administrator. The upgrade migration rejects an already-bootstrapped directory that lacks one. Future credential types, assurance and recovery must explicitly extend this predicate. This checks stored credential state, not possession of the password or an end-to-end authentication attempt. The revoke-all command advances the last administrator's credential epoch without disabling the account or deleting its password verifier.

Epoch advancement is the persistent revocation foundation. Sessions capture the current epoch. Access and [refresh credentials](refresh-tokens.md) bind the original session, and new authorization checks consult authoritative state before accepting them. Reactivation cannot revive an old epoch. A stored password itself is not an issued bearer token. [Self-service termination](sessions.md) revokes one original browser session and its associated token checks without advancing the principal epoch.

The singleton policy revision advances transactionally for principal inserts/updates, capability inserts/updates, administrator membership changes, credential inserts/updates, and password-record inserts/updates/deletes. Implemented catalog and registration changes participate in the same dependency
contract. New security state must join it through reviewed mutation paths. Credential-specific revocation must be checked directly. This revision is not a universal cache-validity proof. Signed 64-bit database counters fail closed on exhaustion.

Security writes take the singleton lock before reading account facts, then lock the principal, compute the transition, and persist its audit. This deliberately serializes low-rate security mutations. It does not write during authorization checks or justify caching positive decisions. Benchmark lock contention before expanding write-heavy workflows. No transaction spans password hashing or external service calls.

Audit rows record the operation, subject, resulting revisions, timestamp and database login role. Supported operations always write their audit in the same transaction. Ordinary DML cannot edit/delete those rows, but a database owner can change schema and privileged SQL can bypass supported workflows. This is not an externally tamper-evident audit system. [Separate database roles](database-authority.md) and authenticated account-command
audit are implemented. Audit export, retention, broader operator attribution, and
restoration to service remain open. Owners can still alter schema and evidence.

## Connection settings

| Variable                       | Contract                                                                                                                                     |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `DARKHORSE_DATABASE_URL`       | Required for operator commands. PostgreSQL URL with TCP host, user, explicit password and database. No query or fragment. At most 4096 bytes |
| `DARKHORSE_DATABASE_POOL_SIZE` | Default 5, range 1–32                                                                                                                        |
| `DARKHORSE_DATABASE_INSECURE`  | Default false. True explicitly disables database TLS for the local isolated development topology                                             |

TLS normally verifies both the server certificate and hostname. Provision the server's trusted CA through SQLx's PostgreSQL TLS configuration, including `PGSSLROOTCERT` when using a private CA. Do not weaken verification for production. The connection adapter fixes the default port to 5432, sets the public schema search path, disables SQL statement logging and applies 5-second statement, 3-second lock, 10-second idle-transaction, and 5-second pool-acquisition limits. These are initial budgets, not measured throughput guarantees. Size aggregate pools across replicas against the database connection budget.

## Verification and packaging

```sh
make test-unit          # no services/settings/files; dependencies already installed
make test-postgres      # disposable Docker database, real transactions and host CLI
make docker-build
make docker-smoke      # disposable database and the built image
```

The integration runner allocates random names, credentials and loopback ports, and cleans up only its own containers/network. Tests create isolated databases inside the disposable server. They verify repeated migrations, bootstrap races, atomic audit rollback, lifecycle epochs, stale revisions, concurrent administrator and credential-revocation protection, uniqueness, foreign keys, immutable identifiers/meaning, and counter failures. Operator smoke checks exercise real hashing, protected stdin, duplicate rejection and missing authentication configuration. `make test-operator-accounts` adds real password verification, shared Redis budgets, actor/revision rechecks, audit rollback and lost commit responses. Unit builds use runtime parameterized SQL rather than compile-time database introspection, so no database or generated query metadata is needed for compilation.

The pinned multi-stage image builds Rust and static SvelteKit assets and runs as UID 10001. It contains the binary, static assets and runtime certificate store, with no Node server or private inputs. `make docker-smoke` checks HTTP/static serving under a read-only filesystem with capabilities dropped. Override `IMAGE` for another local image tag. The first build needs access to image registries and dependency registries. Development database Compose remains separate from the [integrated HTTPS qualification stack](compose.md), which adds file-backed secrets and runtime/operator role separation. [Kubernetes](kubernetes.md) includes two-replica qualification fixtures. Production
qualification remains open for both deployment profiles.

`make coverage-postgres` combines Rust unit, PostgreSQL and host operator execution, writes an HTML report under `target/postgres-coverage/llvm-cov/html`, and enforces the unchanged 100% line target. `make coverage-unit` remains service-free. The CLI suite exercises real terminal paths. Complete combined instrumentation
remains unfinished. The unit-only report leaves database effects uncovered. These commands do not yet pass the full Rust coverage target. Track the remaining combined authored-code qualification in [issue #2](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/2). Passing functional checks is not a full-coverage or production-readiness claim.

## CLI interface transition

See [the CLI guide](cli.md) and [current operator authority matrix](operator-authority.md) for the canonical command groups, confirmation and output contracts. Direct noninteractive mutations require `--yes`. Explicit existing Make/deployment operator targets supply it as their named action. Legacy command spellings retain the same business invariants. The parser and confirmation flag do not establish caller identity or add an authority bypass.
