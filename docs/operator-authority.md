# Local operator authority

This is the authority inventory for the existing operator interface. The Clap
command tree in [the CLI guide](cli.md) preserves these application operations.
Account and catalog commands require per-command administrator authentication. Other
operator commands still use deployment credentials. This does not complete
[#23](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/23).
Container location, Unix UID, command spelling, a supplied actor name, and `--yes`
provide no application authority. `--yes` confirms intent only.

## Implemented command and dependency matrix

All operator commands below can run with HTTP stopped. They use configured
deployment credentials. They do not call an internal HTTP endpoint or start
server maintenance workers. Help/version/invalid syntax use no runtime settings
or service connections.

| Command                                                             | Required authority                                                                                          | Commit or observation boundary                                           |
| ------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `serve`                                                             | Configured runtime features and credentials                                                                 | Per-request HTTP authentication and authorization                        |
| `operator account list`                                             | Database, active limiter, fresh administrator password                                                      | Shared directory query and read audit commit before releasing the page   |
| `operator application list` / `operator client list APPLICATION_ID` | Database, active limiter, fresh administrator password                                                      | Shared catalog query and read audit commit before releasing the page     |
| `operator application create/update`                                | Runtime database, active limiter, fresh administrator password, confirmation/reason and revision for update | Shared registration mutation and both audits commit together             |
| `operator account show ID`                                          | Database, active limiter, fresh administrator password                                                      | Verified read and actor audit commit together                            |
| `operator account deactivate/reactivate/revoke-all ID REVISION`     | Same account authority, expected revision, bounded reason                                                   | Account change and actor audit commit together                           |
| `operator bootstrap`                                                | Nonowner operator database login, unused bootstrap                                                          | Initial account, credential, membership, flag, and audit commit together |
| `operator migrate`                                                  | Database-owner login                                                                                        | Intent, per-migration atomic history/receipt, final batch receipt        |
| `operator migrate inspect OPERATION_ID`                             | Database-owner login                                                                                        | Read-only primary snapshot of original targets and current history       |
| `operator signing status`                                           | Operator database login, canonical origin, wrapping key                                                     | Binding creation or validation, then inventory                           |
| `operator signing generate/import/activate/retire`                  | Same signing authority and expected revision                                                                | Durable intent, atomic binding/change/audit/receipt                      |
| `operator signing inspect OPERATION_ID`                             | Operator journal read privileges                                                                            | Read-only primary snapshot, database configuration only                  |
| `operator limiter status`                                           | Database and runtime Redis access for active state                                                          | Point-in-time enforcement observation                                    |
| `operator limiter fence`                                            | Protected database mutation authority                                                                       | Inactive generation, wait, and audit commit together                     |
| `operator limiter activate`                                         | Database and Redis recovery credential                                                                      | Durable intent, Redis effect, atomic completion receipt                  |
| `operator limiter inspect OPERATION_ID`                             | Operator journal read privileges                                                                            | Read-only primary snapshot, no Redis dependency                          |
| `operator redis status`                                             | Cache and limiter diagnostic credentials                                                                    | Reachability and configuration, no admission decision                    |

Signing mutations record [durable intent and completion](signing-operations.md).
Provider binding, revision checks, lifecycle audit, and receipt share the mutation
transaction. `status` can initialize the binding outside this journal, so it
requires confirmation. Publication and verification-retention waits remain mandatory.

[Migration reconciliation](migration-operations.md) preserves a committed prefix after a later
step fails. It does not provide individually authenticated operator attribution.
Signing, limiter, and migration inspection have no human authentication or per-read audit. The limiter
[activation receipt](limiter-activation.md) records the database credential boundary.
A repeated fence starts another wait. Activation cannot reset an active generation
or skip the wait. All operator commands remain subject to their documented grants.

Canonical and compatibility spellings share one dispatch path and confirmation
policy. No command grants application roles, impersonates users, or bypasses the
domain authorization evaluator. Administrator membership and application-resource
access remain separate concepts. Active platform administrators may perform only
the implemented account operations, [bounded catalog reads](operator-catalog.md)
and [application writes](operator-applications.md). This grants no application-resource
authority. Deployments must restrict database credential access.

## Credential separation and limits

The [Compose](compose.md#topology-and-authority) and [Kubernetes](kubernetes.md)
fixtures use independent nonowner runtime and operator database logins. A
separate migrator workload receives only the schema-owner URL and public CA.
Operator workloads receive the nonowner operator URL and recovery credentials.
Neither nonowner role can migrate or edit audit records. Runtime cannot write
selected administrator/signing/limiter records, and operator grants exclude
browser sessions, tokens, client secrets and unrelated application tables.
The explicit [database grant policy](database-authority.md) rejects unsafe role
topology, denies future objects and rebuilds both allowlists atomically.

This remains partial separation: runtime has broad credential/session DML, and
operator bootstrap privileges can create administrator membership through direct
SQL. Both can append misleading audit records. Deployment credentials identify
a credential boundary, not an individual or operation-specific authorization.
Account CLI authentication is additional to these SQL privileges. Other command
groups still rely on trusted deployment authority.

Deliver secrets through the documented protected files/mounts or existing
deployment configuration. Do not put passwords, tokens, database URLs, wrapping
keys or private signing material in command arguments. Limit container exec,
Pod creation, secret access and database-owner access outside the application.
Host/container root and database owners remain trusted. Database owners can alter
stored audit records. Records are not tamper-proof. Kubernetes execution identity
is external evidence and is not automatically verified inside the CLI.

## Required authority work before new privileged capabilities

[#23](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/23)
must establish and test the following contracts before the administration groups
planned in #25/#26 ship:

- Routine administration needs verified actor authentication, bounded/recent
  assurance appropriate to the operation, and current operation/target grants.
  Reuse shared attempt limits if passwords are verified. A fresh CLI process
  cannot reset an attempt budget. A platform administrator receives no automatic
  access to application resources or impersonation authority.
- Exceptional recovery needs separately provisioned, bounded credentials and an
  approval/lifecycle contract. A reason string, actor argument or environment
  switch is insufficient. No emergency bypass is implemented here.
- Bind grants and actor revocation state to the mutation's coherent authoritative
  transaction. Define in-flight ordering and prove that checks beginning after
  committed revocation/demotion deny the action. Do not cache positive decisions.
- Commit security mutations and authoritative audit together. Audit must record
  the verified actor or identified operator credential, operation, target,
  correlation, relevant revisions, result and required reason without secrets.
  Supplied claims remain unverified. Define retention, read/export access and
  sanitization. Do not silently prune or expose personal audit data.
- Migration, signing, and limiter journals provide durable intent/result recording
  and conservative unknown-outcome handling for their documented scope. Select protected
  out-of-band evidence before claiming recovery with audit storage unavailable.
  No audit-bypass flag or cross-system atomicity claim is permitted.
- Test runtime compromise, stolen operator credentials, forged actor metadata,
  target isolation, self-escalation, concurrent grant reduction, audit failure
  and lost commit responses against actual restricted database roles.

Routine account commands use the password baseline described in
[the account contract](operator-accounts.md). Exceptional recovery, production
privileged assurance level, runtime-compromise containment, and operator audit retention/export policy remain open
decisions in #1/#23. This inventory is not their independent security review.
The design requirements follow [OWASP authorization guidance](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html)
and [logging guidance](https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html).
Database role ownership follows [PostgreSQL privileges](https://www.postgresql.org/docs/18/ddl-priv.html).
