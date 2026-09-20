# Application and client registration

The private administrator API creates separate applications, protected resources,
resource scopes, and confidential web clients. It is available with the enabled
[password portal](authentication.md#configuration-and-development), after migration 0005. The [application-management console](catalog-administration.md) is available after migration 0018. This API is not
OpenID Connect Dynamic Client Registration. Signing keys/JWKS and pending authorization are described in the [provider contract](provider.md). Code/token issuance is described there as well. [Resource introspection credentials](resource-introspection.md) use a separate administrator lifecycle; logout delivery remains future work.

## Authority and consistency

Every request requires a current browser session and current platform-administrator
membership. Application ownership is an accountable contact, not an administrative
grant. Ownership uses a stable principal ID; reads join the current email. Assigning
an owner requires an active account.

Mutations require a sign-in less than five minutes ago. At exactly five minutes,
sign in again. Reads still require a live session and role. Administrator reads do
not refresh the browser session's idle timestamp. A preflight check precedes
generation of IDs or secrets. The committing transaction repeats session, role,
owner, revision, application binding and grant checks, then changes state and
appends an immutable audit event together. Audit failure rolls everything back.

Updates, rotations and retirements require the expected current revision. A race
returns one success and a conflict. Replacing a client ID means creating another
registration; IDs and application/resource relationships cannot be reassigned.
Resources and scope names are immutable in this milestone.

Registry mutations lock the existing primary authority row before actor/entity
checks, following the directory transaction order. Administrative reads share that
fence. This serializes administrative changes, not client-authentication checks.
The client-authentication port reads a fresh primary snapshot on each call and
requires both the application and client to be active. It verifies the presented
secret and returns the current registration. No positive decision is cached in
Redis. Checks started after committed retirement, deactivation or allowance
reduction observe the change; requests already in progress may finish under their
earlier snapshot. Future token/authorization handlers must independently verify
the current user permissions, credential ceilings and resource audience.

The audit records actor ID, target registration ID, event and database time. Client
updates record a `client_updated` event, including when allowances change. Audit
records contain neither credentials nor copies of the profile or redirect URI.
Structured before/after diffs and an audit browsing UI are later work.

## Client profile

- Only confidential web clients using `client_secret_basic` are accepted.
  The token endpoint applies OAuth Basic encoding rules and requires TLS.
- `refresh_tokens` defaults to `false`; explicit `true` enables the
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
  public clients, implicit flow and dynamic registration are unsupported here.
- Each client has an explicit allowlist of at most 32 resource IDs and 128 scope
  IDs in its own application. Every allowed scope must belong to an explicitly
  allowed resource. Empty lists grant no protected-resource access.
- Resource audiences are stable `urn:darkhorse:resource:<uuid>` values.
  Resource scope names use RFC 6749 scope-token characters, at most 100 bytes,
  and are unique within a resource. Provider scopes (`openid`, `profile`,
  `email`, `address`, `phone`, `offline_access`) are reserved for the later
  protocol workflow. Registration allowances do not grant user capabilities.
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
secrets; user passwords retain Argon2id. Secret-bearing types do not implement
`Debug`, and database statement logging remains disabled.

A successful create/rotate response is the only disclosure. Send the secret to the
client application's backend secret store; do not embed it in browser bundles,
local/session storage, URLs, source control or logs. The administrator's one-time
issuance response is distinct from placing credentials in the client frontend.
Read responses contain only secret IDs, creation times and overlap expiration.

Rotation preserves the client ID. Choose an overlap from zero through 300 seconds.
Zero retires the previous secret for authentication immediately. A positive overlap
accepts it only while `now < expires_ms`. A second rotation immediately retires
any older overlap, so there are at most two usable secrets. Explicit retirement is
immediate and may leave no usable secret; a fresh rotation restores one. Retired
and expired records cannot be revived. IDs, verifiers and creation times cannot be
rewritten. Rows are retained for this milestone; archival/retention needs an
operator workflow before sustained high-volume operation.

There are no automatic retries after an uncertain commit. If a create response is
lost, an administrator may need database/operator assistance to locate the
registration until the directory UI exists. If a rotation response is lost, read
the current revision and rotate again. The lost raw secret cannot be retrieved.

## Browser API

All routes use the existing same-origin HTTPS boundary, host-only HttpOnly session
cookie, redacted errors and `Cache-Control: no-store`. Mutations require exact
`Origin` and `X-Darkhorse-CSRF: 1`. The administrator router permits 16 concurrent
requests per process, a ten-second deadline and 32 KiB JSON bodies. PostgreSQL
pool/statement/lock limits also apply. These are bounds, not a measured SLO.

| Route                                                              | Purpose                                                    |
| ------------------------------------------------------------------ | ---------------------------------------------------------- |
| `POST /api/admin/registration`                                     | Execute one tagged registration command.                   |
| `GET /api/admin/applications/{application_id}`                     | Read current application metadata and owner email.         |
| `GET /api/admin/applications/{application_id}/clients/{client_id}` | Read client metadata, allowances and live secret metadata. |

Every command rejects unknown fields. Successful commands return HTTP 200 with
`{"record": {...}}`; only create-client and rotate-secret additionally return
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

`{
  "operation": "create_application",
  "application": {
    "name": "Orders",
    "owner_id": "<existing active principal UUID>",
    "active": true
  }
}`

Then create its resource and scopes, followed by the client:

`{
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
}`

Client updates replace the complete callback and resource/scope allowance lists.
Use the last returned revision; re-read after a 409 and review the new state.
The owner-only operator `account` command can identify existing users until the
directory API/UI is available. No unauthenticated administrator directory is
introduced.

## Deployment and verification

Migrate explicitly before enabling the portal. Keep the Rust HTTP listener behind
the trusted HTTPS proxy and prevent direct public plaintext access. Runtime needs
the existing session/primary authority permissions, SELECT/INSERT/UPDATE on
applications and clients, SELECT/INSERT on resources/scopes, SELECT/INSERT/DELETE
on their client allowance tables, SELECT/INSERT/UPDATE on client secrets, and
INSERT on registration audit plus sequence usage. Migration ownership and audit
update/delete privileges must remain separate from runtime. Database triggers
also enforce immutable identity, grant binding and terminal retirement.

`make test-registration` runs isolated policy, application and fake-transport
tests without services or configuration. `make test-postgres` covers real
constraints, authority changes, atomic audits, secret lifecycle and races.
`make test-browser` adds same-origin registration, explicit grants, concurrent
rotation, retirement and CSRF checks through the actual Rust server over HTTPS.
`make coverage-core` and `DARKHORSE_TEST_BROWSER=true make coverage-integration`
measure their separately documented scopes. The 100% authored-code target and
production qualification requirements remain unchanged.

### Recorded baseline (2026-09-17)

- Release/static builds and the fast checks pass. The final isolated suites have
  149 Rust, frontend and tooling tests. A staged export without private inputs
  installs dependencies from the existing cache and runs unit tests with network
  access denied; first-time dependency download still requires network access.
- PostgreSQL: 23 scenarios plus operator checks. Redis: five infrastructure and
  14 enforcement/login scenarios, including the separately launched process
  helper. Actual HTTPS Chromium registration/login checks pass.
- Docker build and the migration/bootstrap/static/non-root/read-only image smoke
  pass. Enabled-login production topology and load qualification remain pending.
- Pure domain/application coverage is 100% lines, functions and regions.
  Combined Rust units, PostgreSQL, Redis, operator and browser-driven host
  execution cover **98.73% lines, 93.64% regions and 99.61% functions**, with
  **49 lines uncovered**. The unchanged 100% gate fails. Remaining lines are in
  existing terminal/operator, startup, authentication and limiter failure paths.
  Stable Rust does not report branches here; this report does not measure
  JavaScript, browser/tooling or SQL-trigger coverage.
- Registration functional criteria are implemented. [Issue #6](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/6)
  remains open for qualification, alongside the shared foundation coverage work.
