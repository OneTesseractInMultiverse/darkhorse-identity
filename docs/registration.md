# Application and client registration

The private administrator API creates separate applications, protected resources,
resource scopes, and confidential web clients. It is available with the active
[password portal](authentication.md#configuration-and-development), after migration 0005. The [application-management console](catalog-administration.md) is available after migration 0018. This API is not
the standard OIDC client-registration protocol. Signing keys/JWKS and pending authorization are described in the [provider contract](provider.md). Code/token issuance is described there as well. [Resource introspection credentials](resource-introspection.md) use a separate administrator lifecycle. Logout delivery remains future work.

## Authority and consistency

Every request requires a current browser session and current platform-administrator
membership. Application ownership is an accountable contact, not an administrative
grant. Ownership uses a stable principal ID. Reads join the current email. Assigning
an owner requires an active account.

Mutations require a sign-in less than five minutes ago. At exactly five minutes,
sign in again. Reads still require a live session and role. Administrator reads do
not refresh the browser session's idle timestamp. A preflight check precedes
generation of IDs or secrets. The committing transaction repeats session, role,
owner, revision, application binding and grant checks, then changes state and
appends an immutable audit event together. Audit failure rolls everything back.

Updates, rotations and retirements require the expected current revision. A race
returns one success and a conflict. Replacing a client ID means creating another
registration. IDs and application/resource relationships cannot be reassigned.
Resources and scope names are immutable in this milestone.

Registry mutations lock the existing primary authority row before actor/entity
checks, following the directory transaction order. Administrative reads share that
fence. This serializes administrative changes, not client-authentication checks.
The client-authentication port reads a fresh primary snapshot on each call and
requires both the application and client to be active. It verifies the presented
secret and returns the current registration. No positive decision is cached in
Redis. Checks started after committed retirement, deactivation or allowance
reduction observe the change. Requests already in progress may finish under their
earlier snapshot. Token and authorization handlers independently verify current user permissions,
credential ceilings, and resource audience. Client authentication alone grants no
user authority.

The audit records actor ID, target registration ID, event and database time. Client
updates record a `client_updated` event, including when allowances change. Audit
records contain neither credentials nor copies of the profile or redirect URI.
Structured before/after diffs and an audit browsing UI are later work.

## Client profile

- Only confidential web clients using `client_secret_basic` are accepted.
  The token endpoint applies OAuth Basic encoding rules and requires TLS.
- `refresh_tokens` defaults to `false`. Explicit `true` activates the
  [session-bound refresh profile](refresh-tokens.md). Omitting the field from a
  complete client update sets it to `false`.
- Callback URLs must be canonical ASCII absolute HTTPS URLs, including a path
  (`https://app.example/` is valid). Credentials, fragments, wildcards,
  whitespace, backslashes and malformed percent escapes are rejected. URLs a
  parser would normalize are rejected rather than rewritten: register
  `https://app.example/cb`, not an uppercase host, default `:443` port or dot
  segment variant. International hosts must use their ASCII representation.
  There are no DNS lookups, URL fetches or callback reachability probes.
- Matching uses the exact registered string, including query and percent encoding.
  At most eight callbacks, 2,048 bytes each, are accepted. Native loopback HTTP,
  public clients, implicit flow and client-initiated registration are unsupported here.
- Each client has an explicit allowlist of at most 32 resource IDs and 128 scope
  IDs in its own application. Every allowed scope must belong to an explicitly
  allowed resource. Empty lists grant no protected-resource access.
- Resource audiences are stable `urn:darkhorse:resource:<uuid>` values.
  Resource scope names use RFC 6749 scope-token characters, at most 100 bytes,
  and are unique within a resource. Provider scopes (`openid`, `profile`,
  `email`, `address`, `phone`, `offline_access`) are reserved names. The identity
  profile implements the first three. Address, phone, and offline access remain
  unsupported. Registration allowances do not grant user capabilities.
- Application/client/resource display names are trimmed, limited to 100 Unicode
  characters and reject controls. All external IDs use canonical lowercase UUIDs.

Exact callbacks and client authentication follow the
[OAuth security best current practice](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.1),
[OpenID Connect authentication request](https://openid.net/specs/openid-connect-core-1_0.html#AuthRequest),
[client authentication](https://openid.net/specs/openid-connect-core-1_0.html#ClientAuthentication),
and [scope syntax](https://www.rfc-editor.org/rfc/rfc6749.html#section-3.3).
The narrower registration profile above is a project choice.

## Secrets

A secret contains 256 bits from the operating system random generator, encoded as
64 lowercase hexadecimal characters. Storage receives only its purpose-separated
SHA-256 verifier and lifecycle metadata. This fast verifier is for uniformly random
secrets. User passwords retain Argon2id. Secret-bearing types do not implement
`Debug`, and database statement logging remains disabled.

A successful create/rotate response is the only disclosure. Send the secret to the
client application's backend secret store. Do not embed it in browser bundles,
local/session storage, URLs, source control or logs. The administrator's one-time
issuance response is distinct from placing credentials in the client frontend.
Read responses contain only secret IDs, creation times and overlap expiration.

Rotation preserves the client ID. Choose an overlap from zero through 300 seconds.
Zero retires the previous secret for authentication immediately. A positive overlap
accepts it only for `now < expires_ms`. A second rotation immediately retires
any older overlap, so there are at most two usable secrets. Explicit retirement is
immediate and may leave no usable secret. A fresh rotation restores one. Retired
and expired records cannot be revived. IDs, verifiers and creation times cannot be
rewritten. Rows are retained for this milestone. Archival/retention needs an
operator workflow before sustained high-volume operation.

The browser never retries an uncertain mutation automatically. After a lost create
response, use the catalog directory to locate the record and reread its current
state. After a lost secret response, read the current revision and choose an
explicit rotation. The lost raw secret cannot be retrieved.

## Browser API

All routes use the existing same-origin HTTPS boundary, host-only HttpOnly session
cookie, redacted errors and `Cache-Control: no-store`. Mutations require exact
`Origin` and `X-Darkhorse-CSRF: 1`. The administrator router permits 16 concurrent
requests per process, a ten-second deadline and 32 KiB JSON bodies. PostgreSQL
pool/statement/lock limits apply. These are bounds, not a measured SLO.

| Route                                                              | Purpose                                                    |
| ------------------------------------------------------------------ | ---------------------------------------------------------- |
| `POST /api/admin/registration`                                     | Execute one tagged registration command.                   |
| `GET /api/admin/applications/{application_id}`                     | Read current application metadata and owner email.         |
| `GET /api/admin/applications/{application_id}/clients/{client_id}` | Read client metadata, allowances and live secret metadata. |

Every command rejects unknown fields. Successful commands return HTTP 200 with
`{"record": {...}}`. Only create-client and rotate-secret return
`"client_secret"`. Errors use 400 for invalid input, 401 for missing/invalid
authentication, 403 for insufficient authority/recent authentication, 404 for an
unknown or mismatched target, 409 for conflict, and 503 for unavailable storage or
admission. Error bodies never echo supplied values.

### Command shapes

IDs below are placeholders for returned canonical UUIDs. `application` contains
`name`, `owner_id` and `active`. `client` contains `name`, `active`, optional `refresh_tokens`,
`redirect_uris`, `resource_ids`, `scope_ids` and
`token_endpoint_auth_method: "client_secret_basic"`.

| `operation`          | Required remaining fields                                    |
| -------------------- | ------------------------------------------------------------ |
| `create_application` | `application`                                                |
| `update_application` | `application_id`, `revision`, `application`                  |
| `create_resource`    | `application_id`, `name`                                     |
| `create_scope`       | `application_id`, `resource_id`, `name`                      |
| `create_client`      | `application_id`, `client`                                   |
| `update_client`      | `application_id`, `client_id`, `revision`, `client`          |
| `rotate_secret`      | `application_id`, `client_id`, `revision`, `overlap_seconds` |
| `retire_secret`      | `application_id`, `client_id`, `secret_id`, `revision`       |

Example application creation from a signed-in, same-origin administrator tool:

```json
{
  "operation": "create_application",
  "application": {
    "name": "Orders",
    "owner_id": "<existing active principal UUID>",
    "active": true
  }
}
```

Then create its resource and scopes, followed by the client:

```json
{
  "operation": "create_client",
  "application_id": "<returned application UUID>",
  "client": {
    "name": "Orders web",
    "active": true,
    "redirect_uris": ["https://orders.example/callback"],
    "resource_ids": ["<returned resource UUID>"],
    "scope_ids": ["<returned scope UUID>"],
    "token_endpoint_auth_method": "client_secret_basic"
  }
}
```

Client updates replace the complete callback and resource/scope allowance lists.
Use the last returned revision. Re-read after a 409 and review the new state.
The [user directory](console.md) supplies owner selection from current accounts.
The account CLI requires fresh administrator authentication. No public directory
lookup is available.

## Deployment and verification

Migrate explicitly before activating the portal. Keep the Rust HTTP listener behind
the trusted HTTPS proxy and prevent direct public plaintext access. Runtime needs
the existing session/primary authority permissions, SELECT/INSERT/UPDATE on
applications and clients, SELECT/INSERT on resources/scopes, SELECT/INSERT/DELETE
on their client allowance tables, SELECT/INSERT/UPDATE on client secrets, and
INSERT on registration audit. Identity columns need no direct sequence grant.
Use the complete [reviewed database policy](database-authority.md). Migration ownership and audit
update/delete privileges must remain separate from runtime. Database triggers
enforce immutable identity, grant binding and terminal retirement.

`make test-registration` runs isolated policy, application and fake-transport
tests without services or configuration. `make test-postgres` covers real
constraints, authority changes, atomic audits, secret lifecycle and races.
`make test-browser` adds same-origin registration, explicit grants, concurrent
rotation, retirement and CSRF checks through the actual Rust server over HTTPS.
`make coverage-core` and `DARKHORSE_TEST_BROWSER=true make coverage-integration`
measure their separately documented scopes. The 100% authored-code target and
production qualification requirements remain unchanged.

## Recorded evidence

[Verification](verification.md) records the latest consolidated baseline and
coverage limits. Registration tests establish their exercised contracts, not
complete provider conformance, deployment capacity, or production readiness.

## Source reference

[registration policy](../crates/domain/src/registration.rs),
[HTTP input mapping](../crates/adapters/src/registration_http/input.rs).
