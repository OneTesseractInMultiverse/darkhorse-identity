# Persistence and operator bootstrap

This is the first PostgreSQL persistence boundary. It provides administrator bootstrap and trusted operator account operations. Login, issued credentials, HTTP directory administration, and production deployment qualification are separate work. No bootstrap endpoint is exposed over HTTP.

## Local database

Install Docker with Compose, start its engine, then run:

```sh
make db-setup
make db-up
make db-migrate
make bootstrap
```

Setup creates a random database password and connection settings in `.local/database-password` and `.local/database.env`, with owner-only file permissions. Existing credentials are preserved. Incomplete or broadly readable files are rejected. Keep these files private; they are excluded from Git, image build contexts, and release exports. Do not delete credentials while retaining a database volume that uses them.

Percona Server for PostgreSQL 18.6.1 (PostgreSQL 18.6 compatible) listens only on `127.0.0.1:54329`. Compose project names are derived from the workspace path, so separate workspaces own separate volumes; their default host port still conflicts if both run simultaneously. To change the port, update both the port and connection URL in the local settings file. `make db-down` removes this workspace's containers/network and preserves its volume and credentials. `make clean` also preserves them. Reopening the workspace at another path creates a different Compose project. The pinned container and existing-volume migration procedure are documented in [Percona containers](percona.md). Existing upstream volumes require logical restoration into the separate Percona volume before cutover.

Migrations are versioned SQL embedded in the Rust binary. Apply them explicitly with `migrate`; ordinary HTTP startup does not migrate, bootstrap, or connect to PostgreSQL yet. SQLx records migration checksums. Once published, change the schema with a new migration, not by editing an applied migration. Back up and verify restore procedures before migrating valuable data; no automatic down-migration or destructive reset command is provided.

## Bootstrap input and password storage

Interactive bootstrap requests email, first name, last name, and a hidden password with confirmation. Nothing is supplied as a command argument. It requires a terminal. Automated operators can instead pipe protected JSON to:

```sh
./target/release/darkhorse-server bootstrap --stdin
```

Supply exactly `email`, `first_name`, `last_name`, and `password` as JSON strings. Input is limited to 16 KiB and unknown fields are rejected. Use a secret manager or protected input stream; shell command literals and trace logs can retain secrets. The process needs the database settings described below. An explicit password in the database URL also prevents implicit password-file lookup. Errors do not include supplied values, SQL parameters, password verifiers, or connection strings. Success reports only the new principal identifier.

Initial validation policy:

- Email: strip surrounding whitespace, accept ASCII dot-atom local parts and DNS-style domains containing a dot, and bound the whole address to 254 bytes, local part to 64, and domain labels to 63. Preserve display spelling. Compare the entire ASCII address case-insensitively using a unique generated database key. Quoted addresses and internationalized addresses are not accepted. The address is not considered verified merely because it passes syntax validation.
- Names: first and last names are required, trimmed, control-free Unicode strings of 1–100 characters each. Extended profile attributes arrive with profile management.
- Passwords: 15–128 Unicode characters, no control characters. Passwords are neither trimmed nor normalized. The login/recovery work must add its password blocklist and verification/recovery rules.
- Password verifiers: RustCrypto Argon2id v19, 64 MiB, three iterations, one lane, 32-byte output and an independent 16-byte operating-system random salt. The PHC string retains its algorithm parameters. One hashing worker per operator process is admitted before scheduling; this is not the future distributed login limiter. Plaintext working buffers are zeroized where owned, without claiming that every runtime/OS copy can be erased.

The operator prepares identifiers and the verifier before beginning the database transaction. The transaction locks the singleton security state and inserts the principal, credential, administrator membership, audit record, and consumed bootstrap flag together. Concurrent attempts have exactly one winner. Failed transactions leave none of those writes committed. Bootstrap cannot be reset through the supported interface. If a connection fails during commit, inspect database state before retrying: the outcome may be unknown to the client.

## Authority, lifecycle and revisions

These commands require trusted operator database access. They are not user-facing authenticated APIs and do not perform application RBAC checks. The future HTTP adapters must establish caller authority before invoking the application ports. Platform administrator membership is separate from application ownership and does not bypass the pure authorization evaluator.

After building the binary, use `account ID` to read the current record. `deactivate ID REVISION`, `reactivate ID REVISION`, and `revoke-all ID REVISION` require its current revision. The local wrapper can supply development database settings:

```sh
node scripts/database.mjs run account ID
node scripts/database.mjs run revoke-all ID REVISION
```

Replace the placeholders with the returned UUID and revision. Conflicting revisions fail; read current state before deciding whether to retry. A no-op status change leaves revisions and audit unchanged. Real transitions and their audit event commit atomically:

| Change     | Principal revision | Credential epoch |
| ---------- | ------------------ | ---------------- |
| Deactivate | Advance            | Advance          |
| Reactivate | Advance            | Preserve         |
| Revoke all | Advance            | Advance          |

Identifiers are nonzero random UUIDs generated by the adapter. Principal and credential identities cannot change or be hard-deleted through ordinary DML. Capabilities retain immutable IDs, permission keys and meanings; retirement is one-way. Credential records separate their kind from password material so later credential types can be added independently.

The last **eligible** platform administrator cannot be deactivated, including concurrent attempts. Eligibility currently means active membership with at least one nonrevoked password credential and its verifier record. Membership without a credential does not count. Deferred database constraints also reject removal/revocation of the final eligible credential and prevent consuming bootstrap without an eligible administrator. The upgrade migration rejects an already-bootstrapped directory that lacks one. Future credential types, assurance and recovery must explicitly extend this predicate. This checks stored credential state, not possession of the password or an end-to-end authentication attempt. The revoke-all command advances the last administrator's credential epoch without disabling the account or deleting its password verifier.

Epoch advancement is the persistent revocation foundation. Sessions capture the current epoch; access and [refresh credentials](refresh-tokens.md) bind the original session, and new authorization checks consult authoritative state before accepting them. Reactivation cannot revive an old epoch. A stored password itself is not an issued bearer token.

The singleton policy revision advances transactionally for principal inserts/updates, capability inserts/updates, administrator membership changes, credential inserts/updates, and password-record inserts/updates/deletes. Future roles, scopes, bindings, grants and client restrictions must explicitly join that dependency contract. Credential-specific revocation must also be checked directly; this revision is not a universal cache-validity proof. Signed 64-bit database counters fail closed on exhaustion.

Security writes take the singleton lock before reading account facts, then lock the principal, compute the transition, and persist its audit. This deliberately serializes low-rate security mutations. It does not write during authorization checks or justify caching positive decisions. Benchmark lock contention before expanding write-heavy workflows. No transaction spans password hashing or external service calls.

Audit rows record the operation, subject, resulting revisions, timestamp and database login role. Supported operations always write their audit in the same transaction. Ordinary DML cannot edit/delete those rows, but a database owner can change schema and privileged SQL can bypass supported workflows. This is not an externally tamper-evident audit system. Runtime/migration role separation, user actor attribution, audit export/retention, backup/restore and production TLS topology remain deployment/security work.

## Connection settings

| Variable                       | Contract                                                                                                                                     |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `DARKHORSE_DATABASE_URL`       | Required for operator commands; PostgreSQL URL with TCP host, user, explicit password and database; no query or fragment; at most 4096 bytes |
| `DARKHORSE_DATABASE_POOL_SIZE` | Default 5, range 1–32                                                                                                                        |
| `DARKHORSE_DATABASE_INSECURE`  | Default false; true explicitly disables database TLS for the local isolated development topology                                             |

TLS normally verifies both the server certificate and hostname. Provision the server's trusted CA through SQLx's PostgreSQL TLS configuration, including `PGSSLROOTCERT` when using a private CA; do not weaken verification for production. The connection adapter fixes the default port to 5432, sets the public schema search path, disables SQL statement logging and applies 5-second statement, 3-second lock, 10-second idle-transaction, and 5-second pool-acquisition limits. These are initial budgets, not measured throughput guarantees. Size aggregate pools across replicas against the database connection budget.

## Verification and packaging

```sh
make test-unit          # no services/settings/files; dependencies already installed
make test-postgres      # disposable Docker database, real transactions and host CLI
make docker-build
make docker-smoke      # disposable database and the built image
```

The integration runner allocates random names, credentials and loopback ports, and cleans up only its own containers/network. Tests create isolated databases inside the disposable server. They verify repeated migrations, bootstrap races, atomic audit rollback, lifecycle epochs, stale revisions, concurrent administrator and credential-revocation protection, uniqueness, foreign keys, immutable identifiers/meaning, and counter failures. Operator smoke checks exercise real hashing, protected stdin, duplicate rejection and revocation. Unit builds use runtime parameterized SQL rather than compile-time database introspection, so no database or generated query metadata is needed for compilation.

The pinned multi-stage image builds Rust and static SvelteKit assets and runs as UID 10001. It contains the binary, static assets and runtime certificate store, with no Node server or private inputs. `make docker-smoke` checks HTTP/static serving under a read-only filesystem with capabilities dropped. Override `IMAGE` for another local image tag. The first build needs access to image registries and dependency registries. The current Compose file supplies the local database only; complete HTTPS application deployment and Kubernetes packaging have their own issues.

`make coverage-postgres` combines Rust unit, PostgreSQL and host operator execution, writes an HTML report under `target/postgres-coverage/llvm-cov/html`, and enforces the unchanged 100% line target. `make coverage-unit` remains service-free. Interactive terminal paths and some process/error paths still need measured execution; the unit-only report correctly leaves database effects uncovered. These commands do not yet pass the full Rust coverage target. Track the remaining combined authored-code qualification in [issue #2](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/2); passing functional checks is not a full-coverage or production-readiness claim.
