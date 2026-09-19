# Session-bound refresh tokens

Confidential clients can explicitly enable opaque refresh tokens through the
administrator registration API with `client.refresh_tokens: true`. The default is
`false`, including when the field is omitted from a complete client update. An
update increments the client revision and invalidates its existing grants. This
profile uses `client_secret_basic` over TLS and requires the original PKCE code
flow. It does not support public clients, `offline_access`, or machine credentials.

## Client contract

An opted-in code exchange returns an opaque `refresh_token` alongside the opaque
access token and signed ID token. `POST /token` accepts form-encoded
`grant_type=refresh_token` and `refresh_token`, with the same client's HTTP Basic
credentials. Optional `scope` can retain or narrow the immediately preceding
refresh grant. Optional `resource` must match the original resource; omission
preserves that resource. Supplying code-flow parameters in a refresh request is
rejected. The existing body, concurrency, browser-origin and cookie restrictions
apply to both grants.

Every successful refresh returns a new access token and a new refresh token,
`token_type=Bearer`, `scope`, and `expires_in`. Refresh responses omit `id_token`;
the original authentication event does not change. Responses and errors carry
`Cache-Control: no-store` and `Pragma: no-cache`.

| Limit            | Behavior                                                                                        |
| ---------------- | ----------------------------------------------------------------------------------------------- |
| Family lifetime  | Ends eight hours after the original browser authentication                                      |
| Member lifetime  | At most 15 minutes after issuance, capped by family expiry                                      |
| Access lifetime  | At most five minutes, capped by family expiry                                                   |
| Family size      | At most 256 refresh members, including the initial one                                          |
| Session validity | Original session, credential epoch, active account and current registration must still be valid |

Refresh never updates the browser session's idle timestamp. An idle browser
session therefore expires even if its application keeps refreshing credentials.
The client must then start a new authorization flow; a still-valid SSO session
can avoid another password prompt. A full family also requires new authorization.
These are session-bound credentials, not offline access.

### Concurrency, retries and revocation

A client must serialize refresh requests for each family and replace its stored
refresh token atomically with a successful response. Reusing a consumed token
while its family is live revokes that family, including every access token issued
from it. Concurrent requests have one committed winner; the losing replay then
invalidates the winner's credentials. Other families remain independent. [Ending the original browser session](sessions.md)
also denies all families bound to it; other browser sessions remain independent.

There is no replay grace period or response recovery cache. If a response is lost
and the outcome is uncertain, start a new authorization flow. Retrying a token
whose rotation committed revokes the family. A transaction that rolls back leaves
the original token unconsumed, but a disconnected client cannot infer rollback.

Authenticated refresh-token revocation at `/revoke` revokes the entire family.
Revoking one access token remains access-only. Authorization-code replay also
revokes the corresponding family. Wrong-client, wrong-issuer and wrong-resource
requests do not revoke another grant. A bad or expired client secret fails before
any family action, including when it expires during a lock wait.

Refresh credentials are not accepted at UserInfo or returned as active by either
introspection profile. `token_type_hint` never changes a credential's actual
purpose. Keep refresh tokens in a confidential application backend; browser
JavaScript and local storage must not receive them. The browser holds only its
application's secure, HttpOnly session cookie.

## Persistence and authority

Refresh tokens contain 256 OS-generated random bits with a `dr_` prefix. Only
purpose-bound SHA-256 verifiers are persisted. Immutable family records bind the
issuer and root authorization code; the code binds client, principal, original
session, resource and consent ceiling. Each generation can only retain or narrow
the preceding generation's scopes and capability ceiling. Restoring a removed
permission cannot expand a later generation that already dropped it.

The PostgreSQL adapter owns one transaction around current policy reads,
consumption, replacement credentials and audit. It first acquires the shared
security fence in a separate statement, then locks the root code. Subsequent
primary reads use fresh snapshots. Family operations and access checks use that
same root lock; no positive decision cache or read replica authorizes access.
Time-sensitive client/session/member checks run again after blocking reads.
Initial code exchanges also recheck client, session and code expiry after signing.
Storage and audit failures roll back the whole transition and fail closed.

Database constraints enforce bounded generations, one unconsumed member, one
access token per generation, immutable bindings, shrinking ceilings and terminal
revocation. Audit events record family UUID, generation, principal and client;
they contain no raw token or verifier. Consumed verifiers remain available for
replay detection until the entire family is eligible for cleanup.

## Migration and maintenance

Apply migration `0012_refresh_rotation.sql` with the existing migration command
before running this version against an existing database. Use a maintenance
window: the migration changes uniqueness constraints and triggers used by token
issuance, and mixed old/new application versions are not supported. Existing
clients remain opted out and existing access tokens retain their original expiry.
Back up production data and validate the upgrade on a restored copy before rollout.

Families become eligible for deletion 24 hours after their absolute expiry,
including revoked families. A database batch deletes at most ten families and
cascades their refresh/access rows. Each family is limited to 256 of each, so the
maximum child work is bounded. Ordered root locks with `SKIP LOCKED` let concurrent
workers skip busy families. Audit and authorization-code rows remain retained;
legacy access tokens, codes, consent and audit still need separate retention work.

Provider-enabled server processes run periodic cleanup. Each minute they attempt
at most ten batches, stopping early when a batch is not full. Transactions release
locks between batches. Shutdown cancels pending cleanup with transaction rollback.
An unavailable sweep reports a fixed diagnostic without credential data and retries
at the next interval. Operators must size and monitor storage: this bound is not a
throughput guarantee, and cleanup capacity/lag monitoring remains qualification work.

## Verification and limits

`make test-refresh` runs isolated policy/material/transport tests. `make
test-postgres` exercises real rotation, replay, rollback, expiry, lock waits,
permission changes, family bounds and concurrent cleanup. `make test-browser`
verifies the HTTPS registration/code/refresh flow, narrowed UserInfo, purpose
isolation, lost response and revocation using a confidential reference client.

The functional implementation does not establish OIDC conformance, complete
coverage, sustained performance or release readiness. [Issue #12](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/12) retains those
qualification items, including process restart during transactions, maintenance
shutdown/failure injection and storage growth under sustained load. Existing
shared token-route abuse controls and broader release work remain tracked separately.

Protocol basis: [OAuth refresh requests](https://www.rfc-editor.org/rfc/rfc6749.html#section-6),
[refresh-token security](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.14),
and [OIDC refresh responses](https://openid.net/specs/openid-connect-core-1_0.html#RefreshTokenResponse).
