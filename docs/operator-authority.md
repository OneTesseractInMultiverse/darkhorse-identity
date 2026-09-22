# Local operator authority

This is the authority inventory for the existing operator interface. The Clap
command tree in [the CLI guide](cli.md) preserves these application operations.
It does not authenticate an individual administrator or complete
[#23](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/23).
Container location, Unix UID, command spelling, a supplied actor name, and `--yes`
provide no application authority. `--yes` confirms intent only.

## Implemented command and dependency matrix

All operator commands below can run with HTTP stopped. They use configured
deployment credentials; they do not call an internal HTTP endpoint or start
server maintenance workers. Help/version/invalid syntax use no runtime settings
or service connections.

| Command                                                         | Required access and state                                                          | Existing invariants / audit boundary                                                                                                                                                                                                                                                                                                              |
| --------------------------------------------------------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `serve`                                                         | Runtime configuration; enabled features determine database, Redis and key access   | Existing HTTP authentication/authorization and admission policies; runtime has broad application DML privileges.                                                                                                                                                                                                                                  |
| `operator account show ID`                                      | Trusted database read access                                                       | Returns the selected profile/status/revision, never password material. No individual actor authentication or read audit currently exists.                                                                                                                                                                                                         |
| `operator account deactivate/reactivate/revoke-all ID REVISION` | Trusted database mutation access                                                   | Expected revision, shared security transaction fence, last eligible administrator protection and credential epochs. State and existing security audit commit together; the audit identifies the target, not a verified human operator.                                                                                                            |
| `operator bootstrap`                                            | Database owner access; unconsumed bootstrap state                                  | Exactly one initial eligible administrator; credentials, membership, bootstrap flag and security audit in one transaction. Password input is protected. No supported bootstrap reset.                                                                                                                                                             |
| `operator migrate`                                              | Schema owner/migration authority                                                   | SQLx migration checks/history. This is not an individually authenticated management operation or a complete operator intent/result audit. Stop serving and serialize deployment changes as documented.                                                                                                                                            |
| `operator signing status/generate/import/activate/retire`       | Provider origin/configuration, wrapping key and trusted database access            | Every command validates or initially creates the issuer/wrapping-key binding. Therefore even `status` requires confirmation. Existing revision, publication and verification-retention rules apply; key lifecycle mutation/audit is atomic. Binding creation precedes the lifecycle transaction and has no complete operator intent/result audit. |
| `operator limiter status`                                       | Database; runtime limiter Redis credentials when active                            | Observes authoritative generation and validated counters. Does not grant admission or establish a reusable authorization decision.                                                                                                                                                                                                                |
| `operator limiter fence`                                        | Protected database mutation authority                                              | Durable inactive generation, mandatory recovery wait and limiter audit commit together. Repeated fencing starts another wait.                                                                                                                                                                                                                     |
| `operator limiter activate`                                     | Protected database authority and separately provisioned Redis recovery credentials | Initializes the waited generation and then records its active identity. PostgreSQL and Redis effects are not atomic together; uncertainty remains restrictive and requires inspection. No skip-wait or reset-active-generation option.                                                                                                            |
| `operator redis status`                                         | Cache and limiter diagnostic credentials                                           | Reports role-specific reachability/configuration and public process metadata, not enforcement readiness. No individual operator/read audit.                                                                                                                                                                                                       |

Canonical and compatibility spellings share one dispatch path and confirmation
policy. No command grants application roles, impersonates users, or bypasses the
domain authorization evaluator. Administrator membership and application-resource
access remain separate concepts. Account commands do not establish caller RBAC;
deployments must restrict who receives their database credential.

## Credential separation and limits

The [Compose](compose.md#topology-and-authority) and [Kubernetes](kubernetes.md)
fixtures give the runtime a nonowner database login and separate operator
workloads the schema-owner/recovery credentials. The runtime cannot migrate or
write selected administrator/signing/limiter records. The explicit
[database grant policy](database-authority.md) denies access to unlisted objects,
rejects unsafe runtime role topology, and permits application audit append/read
without edit or deletion privileges. Runtime still has broad DML
access to sensitive credential/session tables and can append misleading audit
records; this is **partial
separation**, not containment of a compromised runtime. A shared owner credential
does not identify an individual or provide operation-specific authorization.

Deliver secrets through the documented protected files/mounts or existing
deployment configuration. Do not put passwords, tokens, database URLs, wrapping
keys or private signing material in command arguments. Limit container exec,
Pod creation, secret access and database-owner access outside the application.
Host/container root and database owners remain trusted; database owners can alter
stored audit records. Records are not tamper-proof. Kubernetes execution identity
is external evidence and is not automatically verified inside the CLI.

## Required authority work before new privileged capabilities

[#23](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/23)
must establish and test the following contracts before the administration groups
planned in #25/#26 ship:

- Routine administration needs verified actor authentication, bounded/recent
  assurance appropriate to the operation, and current operation/target grants.
  Reuse shared attempt limits if passwords are verified; a fresh CLI process
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
  sanitization; do not silently prune or expose personal audit data.
- For migrations and cross-system recovery, add durable intent/result recording,
  reconciliation and conservative unknown-outcome handling. Select protected
  out-of-band evidence before claiming recovery with audit storage unavailable.
  No audit-bypass flag or cross-system atomicity claim is permitted.
- Test runtime compromise, stolen operator credentials, forged actor metadata,
  target isolation, self-escalation, concurrent grant reduction, audit failure
  and lost commit responses against actual restricted database roles.

The authentication/recovery mechanism, privileged assurance level, complete
database-role separation, and operator audit retention/export policy remain open
decisions in #1/#23. This inventory is not their independent security review.
The design requirements follow [OWASP authorization guidance](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html)
and [logging guidance](https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html);
database role ownership follows [PostgreSQL privileges](https://www.postgresql.org/docs/18/ddl-priv.html).
