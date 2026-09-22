# Database authority and audit retention

The deployment grants in [grant-runtime.sql](../deploy/grant-runtime.sql) use an
explicit allowlist for the dedicated application database. Apply them with HTTP
serving stopped, after reviewed migrations, as `darkhorse_owner` or the cluster
administrator. Compose `make stack-migrate` applies this script; Kubernetes
operators apply it explicitly after migration. It is a deployment policy, not a
schema migration or an individually authenticated CLI authorization mechanism.

## Runtime boundary

| State                                                                                 | Runtime privileges                                                                                                                          |
| ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Named application tables                                                              | Existing SELECT, INSERT, UPDATE and DELETE privileges needed by application workflows; application checks and database triggers still apply |
| Application audit tables                                                              | SELECT and INSERT; no UPDATE, DELETE, TRUNCATE, trigger changes, direct sequence access or grant option                                     |
| Administrator membership, signing keys, limiter authority and provider/limiter audit  | SELECT only                                                                                                                                 |
| Personal-key lifetime policy                                                          | SELECT only; changes require trusted deployment authority                                                                                   |
| Security fence                                                                        | SELECT and UPDATE of `policy_revision`; cannot change the bootstrap flag                                                                    |
| Authorization capacity fence                                                          | SELECT and UPDATE of `singleton`, sufficient for PostgreSQL row locks                                                                       |
| Provider binding                                                                      | SELECT, INSERT and UPDATE of `last_ms`; cannot change issuer, wrapping fingerprint or key revision                                          |
| Login-budget, email-delivery and object-storage bindings                              | SELECT and INSERT for existing startup validation; immutable-record triggers preserve existing bindings                                     |
| Four named pure constraint validators                                                 | EXECUTE with invoker privileges; no elevated rights                                                                                         |
| Migration history, unlisted tables, sequences and other callable application routines | No runtime privileges                                                                                                                       |

Existing trigger functions still execute with the invoking transaction's
privileges. CHECK constraints need execution of `oidc_identity_scopes`,
`oidc_identity_claims`, `capability_ceiling_valid` and `resource_token_scopes`;
these are the only routine grants.
Identity columns allocate audit identifiers without direct sequence grants.
No runtime operation receives schema/database creation, temporary tables,
ownership, role-management or permission-delegation authority from this script.

The script rebuilds direct and PUBLIC grants in one transaction, including prior
column grants. It also clears global and `public`-schema default grants for
objects created by `darkhorse_owner`, including PostgreSQL's default PUBLIC
routine execution. A later table, sequence or routine receives no runtime access
merely because the script runs again. Review migrations and their explicit grant
changes together. Migrations must use the designated owner and must not introduce
unreviewed grants, other schemas or elevated routines.

Unsafe runtime role attributes, role memberships (including membership that only
allows `SET ROLE`), and database/schema/relation/routine ownership cause the script
to fail. Resolve the deployment configuration while serving remains stopped; do
not work around it by sharing the owner credential. Missing required relations
or dependent grants also abort the transaction. An interrupted connection can
leave the commit outcome unknown: inspect privileges and reapply the reviewed
script before restarting. Never infer successful application from a lost reply.

## Audit access and limitations

There is no automatic audit deletion, retention deadline or public export
endpoint. Runtime reads remain necessary for existing bounded admission queries;
its credential can also read personal audit data. Restrict secret mounts, exec
access and backups accordingly. Owner-operated exports need protected storage,
explicit recipients and a deployment retention policy; ordinary console or CLI
access does not grant an audit export capability.

Audit-edit privileges are denied independently of existing immutable-row
triggers. When an account operation cannot insert its audit record, its state
change rolls back. This does **not** establish tamper-proof evidence: database
owners and host administrators remain trusted. A compromised runtime can append
misleading records and alter the application tables it can write. A stored
database role identifies a credential boundary, not an authenticated human, and
caller-supplied fields are not independent provenance.

This remains partial separation. Runtime credentials still permit broad credential/session writes.
Account CLI operations additionally require a fresh administrator password and
transactional actor audit, described in [the account contract](operator-accounts.md).
Operator and migration workloads still share the schema-owner credential.
Narrower database authority, protected emergency
credentials, independent audit evidence and cross-system recovery recording
remain in [#23](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/23).
No audit-bypass or emergency-access mechanism is introduced here.

## Verification

`make test-db-authority` creates its own disposable Percona database and uses real
password-authenticated owner/runtime connections. It rejects a wrong password,
checks SQLSTATE permission failures independently of row triggers, denies new
objects and role escalation, verifies grant reapplication/rollback, and exercises
direct SQL transactions with allowed and denied audit insertion, plus account CLI
refusal without authentication. Actual authenticated account operations and
transactional audit failures are covered by `make test-operator-accounts`. It also runs
inside `make test-postgres` and hosted source verification. These are integration
tests; `make test-unit` remains service-free.

Compose and Kubernetes qualification exercise the same grant script with the
packaged runtime. Local development uses separate trusted credentials and does
not establish this restricted-role boundary. SQL/process evidence is separate
from Rust/frontend line coverage; it does not complete the authored-coverage gate.

The policy follows PostgreSQL's [privilege model](https://www.postgresql.org/docs/18/ddl-priv.html)
and [revocation rules](https://www.postgresql.org/docs/18/sql-revoke.html), and
OWASP's [logging protection guidance](https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html).
