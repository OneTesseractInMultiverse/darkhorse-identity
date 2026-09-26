# Authenticated operator authority

Every password-authenticated operator invocation authorizes one bounded command.
The password proof is neither a session nor a reusable administrative credential.
Account and catalog commands share this contract, including responses that reveal
whether a target exists or has changed. Deployment-credential operations such as
migration, signing-key management and recovery have a separate contract in
[database authority](database-authority.md).

## Transaction and disclosure boundary

The [application service](../crates/application/src/operator_accounts.rs) charges
shared login admission, reads a candidate from the primary, verifies the password
outside the operation transaction, and constructs a private proof. The proof's
60-second lifetime begins at the candidate read; hashing and lock waits consume
that lifetime. No positive authorization result is cached.

Inside the transaction, every authority check reads the same exact password
credential and verifier, its nonrevoked state, principal status and credential
epoch, administrator membership, and current database time. Time moving backward
or reaching the proof's expiry denies the operation. The
[domain policy](../crates/domain/src/operator_accounts.rs) defines proof validity,
protected outcomes and permitted completion states. The
[PostgreSQL coordinator](../crates/adapters/src/postgres/operator_accounts.rs)
performs the reads and holds the locks.

Supported security writers acquire the exclusive security fence. Account detail,
catalog and directory readers hold its shared mode; account and catalog
mutations hold its exclusive mode. An operation that acquires the fence first
can complete before a waiting reduction. Checks beginning after that reduction
commits use the reduced authority. Proof expiry remains relevant while the fence
is held, so readers check after their queries and all protected outcomes check
again after the audit write, immediately before attempting commit.

The last authority check is a transaction decision, not a guarantee that a proof
remains live until stdout is consumed. Time can pass during commit or output, and
a new revocation can commit after the operation releases its fence. No protected
result is released until commit is acknowledged. Commit acknowledgement loss
returns an unknown outcome without a record or automatic retry. A broken output
stream can follow a committed operation; it does not undo that operation.

Database owners remain trusted. Deferred owner-defined triggers or writers that
ignore the documented fence are outside this ordering contract. Test triggers
are controlled fault injection used to exercise rollback and disclosure checks;
they are not evidence of an untrusted role exploiting those owner capabilities.

## Command and outcome matrix

“Final check” below means a fresh authority check after audit and before commit.
Preflight input errors occur before target access and are separate from errors
derived from authenticated database state.

| Commands and outcomes                                                                                               | Fence and checks                                                                                                                     | Implementation                                                                                                                                | Executable evidence                                                                                                                                                                                                                                |
| ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `account list`: page, including empty page                                                                          | Shared; before query, after query, final check                                                                                       | [Directory reader](../crates/adapters/src/postgres/operator_directory.rs)                                                                     | [Directory ordering and failure tests](../crates/adapters/tests/integration/operator_directory.rs), [authority matrix](../crates/adapters/tests/integration/operator_authority.rs)                                                                 |
| `account show`: record or not found                                                                                 | Shared; separate post-fence target read, after target read, final check                                                              | [Account coordinator](../crates/adapters/src/postgres/operator_accounts.rs)                                                                   | [Account tests](../crates/adapters/tests/integration/operator_accounts.rs), [authority matrix](../crates/adapters/tests/integration/operator_authority.rs)                                                                                         |
| `account deactivate`, `reactivate`, `revoke-all`: change or no-op                                                   | Exclusive; before target lock, after change preparation and before writing, after operation, final check                             | [Account coordinator](../crates/adapters/src/postgres/operator_accounts.rs), [directory change](../crates/adapters/src/postgres/directory.rs) | [Self-change, expiry, suppression and concurrent last-administrator tests](../crates/adapters/tests/integration/operator_authority.rs)                                                                                                             |
| Account mutations: not found, revision conflict, policy rejected                                                    | Exclusive; initial check, completion check for the state-dependent error, final check                                                | [Account coordinator](../crates/adapters/src/postgres/operator_accounts.rs)                                                                   | [Authority matrix](../crates/adapters/tests/integration/operator_authority.rs)                                                                                                                                                                     |
| `application`, `client`, `resource`, `scope`, `role`, `capability` lists: page or missing application               | Shared; before query, after query, final check                                                                                       | [Catalog reader](../crates/adapters/src/postgres/operator_catalog.rs)                                                                         | [Catalog tests](../crates/adapters/tests/integration/operator_catalog.rs), [access catalog tests](../crates/adapters/tests/integration/operator_access_catalog.rs), [authority matrix](../crates/adapters/tests/integration/operator_authority.rs) |
| `application show`, `client show`: configuration or not found, including wrong application                          | Shared; before query, after query, final check                                                                                       | [Detail reader](../crates/adapters/src/postgres/operator_catalog_details.rs)                                                                  | [Detail authority and fault tests](../crates/adapters/tests/integration/operator_catalog_details.rs)                                                                                                                                               |
| `client secret list`: metadata page or not found, including wrong application                                       | Shared; before query, after query, final check                                                                                       | [Credential inventory reader](../crates/adapters/src/postgres/operator_client_secrets.rs)                                                     | [Credential tests](../crates/adapters/tests/integration/operator_client_secrets.rs), [authority matrix](../crates/adapters/tests/integration/operator_authority.rs)                                                                                |
| Application create/update, client update, secret retirement: success or target-dependent invalid/not-found/conflict | Exclusive; before operation, after validation before writes and after writes on success; final check includes state-dependent errors | [Shared mutation transaction](../crates/adapters/src/postgres/operator_mutations.rs)                                                          | [Application](../crates/adapters/tests/integration/operator_applications.rs), [client](../crates/adapters/tests/integration/operator_clients.rs), [retirement](../crates/adapters/tests/integration/operator_client_secrets.rs) tests              |

The common `needs_current_authority` policy covers successful outcomes and
authenticated `Invalid`, `NotFound`, `Conflict` and `PolicyRejected` errors.
Generic denial, admission limits, unavailable dependencies and unknown outcomes
disclose no protected target state. Unit tests enumerate every error variant.
Commands retain application scoping even when the target identifier exists in a
different application. Application ownership alone confers no platform authority.

## Intentional self-changes

A successful self-revocation expects the actor to remain active with exactly one
epoch increment. A successful self-deactivation expects the actor to become
inactive with exactly one epoch increment. These expected states come from the
original proof and requested operation, rather than from the resulting database
record. No-op operations, failures and changes to another principal expect the
original active state and epoch.

The exception is limited to those status and epoch transitions. Both completion
checks still require the same nonrevoked password credential and verifier, retained
administrator membership, and a live original proof. An additional epoch change,
credential replacement/revocation, demotion or expiry rolls back the operation.
A successful self-deactivation therefore reports its own result but cannot
authorize a subsequent invocation. Self-revocation retains the password and
allows a later fresh password proof.

The existing expected-revision and last-eligible-administrator policies apply
before persistence. Concurrent self-deactivations cannot deactivate the last
eligible administrator. A change affecting an unrelated administrator does not,
by itself, invalidate the current actor's authority.

## Audit and failure outcomes

Each successful principal update and required security-audit insert must affect
exactly one row. Suppressed writes, audit errors or missing audit rows prevent
success. The operator audit commits with the change; neither can commit alone.

| Failure boundary                                                                           | Durable state and output                                                                              |
| ------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| Invalid input, missing configuration or admission failure                                  | No target access or operator transaction; bounded input/configuration/limit response                  |
| Authentication failure or authority denial before effects                                  | Existing generic denial-audit path may commit; no protected target result                             |
| Proof expires after a target wait, before account persistence                              | No account write; generic denial audit may commit                                                     |
| Completion authority check fails after a protected result or effect, including after audit | Transaction rolls back, including its audit and any mutation; generic denial only                     |
| Required write or audit fails or is suppressed                                             | Transaction rolls back; unavailable response                                                          |
| Commit acknowledgement fails                                                               | Outcome unknown; no protected record and no automatic retry; inspect correlation ID and primary state |
| Output stream fails after commit                                                           | Committed state/audit remain; nonzero output-failure exit status; no automatic retry                  |

Catalog mutations use an inner savepoint. A failure rolls back that savepoint
before the outer outcome audit, so a generic denial returned inside the mutation
can leave a durable denial audit with no mutation. Failure of the final authority
check always rolls back the outer transaction and its audit.

Denial auditing is consequently not a complete log of process attempts. Account
effects and a failed completion audit share the transaction that must roll back.
This implementation does not add a second, independently committed denial log.

## Qualification and cost

The [authority integration matrix](../crates/adapters/tests/integration/operator_authority.rs)
uses real PostgreSQL transactions for proof expiry, target waits, self-transitions,
concurrent last-administrator changes, unrelated reductions, suppressed effects
and a lost commit acknowledgement. The catalog suites additionally exercise
missing and cross-application references, credential/verifier changes, demotion,
status changes and epoch changes during audit. Nontransactional fixture sequences
prove that expiry tests reached the intended boundary before rolling back.

[Native restricted-role tests](../crates/adapters/tests/integration/operator_process/authority.rs)
exercise the actual binary, runtime grants, password hashing and shared admission.
They assert bounded generic errors without records, rollback on authority loss
and suppressed effects, intentional self-changes, and one committed change after
a broken stdout. The [existing process suite](../crates/adapters/tests/integration/operator_process.rs)
covers runtime audit permissions, admission sharing and credential metadata
isolation. These tests complement, rather than replace, packaged deployment and
production workload qualification.

Relative to the preceding implementation, each additional authority check issues
three top-level SQL statements: exact credential/principal recheck with row locks,
membership lookup, and database time. No additional table, index or cache is used.

| Path                                                      | Additional statements per protected outcome            |
| --------------------------------------------------------- | ------------------------------------------------------ |
| Account list                                              | 3 after audit                                          |
| Account show; account not-found/conflict/policy rejection | 6 across completion checks                             |
| Account change/no-op after successful preparation         | 9: 3 before persistence and 6 across completion checks |
| Catalog lists and secret inventory: successful page       | 0                                                      |
| Catalog lists and secret inventory: not found             | 3 after audit                                          |
| Application/client detail: record or not found            | 0                                                      |
| Catalog mutations: success                                | 0                                                      |
| Catalog mutations: target-dependent error                 | 3 after audit                                          |

These are source-derived top-level statement counts, not measured wire round trips.
Row-count validation adds no statement. The checks extend the existing fence and
row-lock holding interval through more primary reads. Account show now takes a
shared fence and reads its target without a separate update lock. Its authority
queries and audit/commit ordering remain unchanged. The focused comparison and
remaining #32 qualification are recorded in [administrative read measurements](performance-admin-reads.md).
Use the comparable operator workload described in
[performance](performance.md) to measure the cost, preserving admission, audits,
pool limits and strict post-commit denial. Dated results and limitations belong in
[verification](verification.md) and the issue's revision-linked evidence.
