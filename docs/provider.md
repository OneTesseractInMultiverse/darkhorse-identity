# Pending OpenID Connect authorization

This milestone implements protected signing-key management, public JWKS, and
browser-bound pending authorization transactions. It does **not** issue authorization
codes, ID tokens, access tokens or refresh tokens. Approval displays an unavailable
connection; the pending request remains usable only through its server-side port
until its five-minute deadline. It is not a working SSO integration yet.

`/.well-known/openid-configuration` deliberately returns HTTP 503. Discovery for
an authorization-code provider requires a token endpoint; advertising one before
it exists would mislead clients. [Issue #8](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/8)
must connect one-time code issuance/redemption and token profiles before successful
discovery is enabled. [Issue #7](https://github.com/OneTesseractInMultiverse/darkhorse-identity/issues/7)
remains open for that metadata integration and qualification. No OIDC conformance
or production-readiness claim is made.

## Configuration and development

Apply embedded migrations explicitly with `make db-migrate`. Prepare the
[password portal](authentication.md#configuration-and-development), including its
safely recovered limiter. Then:

```sh
make provider-setup
make signing-status
make signing-generate REVISION=0
# Wait at least 60 seconds; use the returned key ID and current revision.
make signing-activate KID=<public-key-id> REVISION=1
make dev-provider
```

`provider-setup` creates `.local/signing-wrap.key` with owner-only permissions and
preserves an existing file. The operator targets read that file and the local
protected database configuration. `dev-provider` starts the HTTPS proxy, frontend
and Rust server with login and pending authorization enabled. Ordinary `make dev`
and `make dev-login` retain their prior behavior. Existing local deployments are
not automatically migrated or enabled by tests/builds.

Outside these helpers, set `DARKHORSE_PROVIDER_ENABLED=true` and
`DARKHORSE_SIGNING_WRAP_KEY` to a separate, uniformly random 256-bit key encoded as
64 lowercase hex characters. Keep this key in deployment secret storage, separate
from database backups, the login budget key and TLS keys. Enabled serving also
requires `DARKHORSE_LOGIN_ENABLED=true` and its database/Redis configuration.

The canonical issuer is the HTTPS origin from `DARKHORSE_PUBLIC_ORIGIN`, serialized
without a trailing slash. Its exact value and a purpose-separated digest of the
wrapping key are pinned in PostgreSQL. A conflicting replica fails startup.
Changing the issuer or wrapping key requires a future explicit migration/recovery
workflow; replacing an environment value cannot silently change either.
`Forwarded` and `X-Forwarded-*` headers never establish issuer trust. The HTTPS
proxy must forward the canonical Host header and restrict direct backend access.

## Signing keys

- AWS-LC generates RSA keys with a 3072-bit modulus and exponent 65537. Imports
  accept the same profile as binary PKCS#8 DER, through bounded standard input:
  `make signing-import REVISION=<revision> < private-key.der`. PEM and secret
  command-line arguments are unsupported. Imports are at most 8192 bytes.
- The intended signature algorithm is RS256. Key IDs are SHA-256 JWK thumbprints
  over canonical public `e`, `kty`, and `n` members. `/jwks` publishes only
  `kid`, `kty`, `n`, `e`, `use=sig`, and `alg=RS256`.
- Private material is encrypted with AES-256-GCM and an OS-generated 96-bit nonce.
  Authenticated data binds the envelope to this issuer, key ID and format version.
  Nonces remain uniquely reserved even after retirement. The database receives
  ciphertext; the wrapping key remains outside it. Decryption rejects envelope
  tampering, issuer/key substitution and a changed public projection.
- A staged key is published for at least 60 seconds before activation. Activation
  atomically makes the previous active key retiring for 600 seconds. There is one
  active key and at most four nonretired keys. Revision checks serialize concurrent
  operator changes; uncertain failures are not retried automatically.
- `make signing-retire KID=<id> REVISION=<revision>` can remove an unused staged
  key immediately, or a retiring key after its verification window. It cannot
  retire the active key. Retiring public keys disappear from JWKS at the deadline;
  explicit retirement erases their encrypted private material. Public identities
  and audit records are retained and cannot be reused or rewritten.
- Reserved message lifetimes are at most 300 seconds, within the 600-second
  verification overlap. ID-token headers use `typ=JWT`; back-channel Logout Tokens
  use `typ=logout+jwt`. Future issuing/validating adapters must keep their claim
  schemas separate: ID tokens bind nonce/authentication context, while Logout
  Tokens require the logout event and session/subject targeting and prohibit nonce.
  This milestone does not yet implement either JWT claim validator or issuer.
  Access and refresh credentials remain opaque in the planned flow.

Key operations and their audit write commit together; failed audit writes roll back
all lifecycle changes. The initial implementation uses non-FIPS AWS-LC and does not
claim FIPS certification. Emergency compromise revocation, KMS/HSM integration,
wrapping-key rotation, retained-record archival and recovery drills remain required
release work. Staging/retirement is an ordinary rotation workflow, not emergency
key revocation. JWKS responses currently use `no-store`; no positive authorization
decision or private key is cached in Redis.

## Authorization request profile

`GET /authorize` accepts a bounded query for an active, administrator-registered
confidential client. HEAD does not create transactions. The supported pending
profile is `response_type=code`, query response mode, and mandatory PKCE S256.
The challenge must be canonical unpadded base64url encoding of exactly 32 bytes.
Plain PKCE, implicit/password grants, request objects, request URIs, claims,
`id_token_hint`, `acr_values`, `login_hint` and `select_account` are unsupported.
No supplied URI is fetched.

Callback matching uses the exact registered string, including query encoding.
Callbacks containing reserved response query names (`code`, `state`, `error`,
`error_description`, `error_uri`, `iss`) cannot start a transaction. State and nonce
are optional, immutable, and bounded to 512 and 256 printable ASCII bytes respectively;
clients should supply unpredictable state and nonce. The server returns accepted
state verbatim through URL encoding on its safe error redirects. Clients remain
responsible for state, issuer and nonce checks in the eventual complete flow.

`openid` is required. Other provider scopes are unavailable until their claims and
endpoints exist. Resource scopes require one explicitly allowed resource audience
and an explicitly allowed scope on that resource in the same application. Scope
names are unique, at most 32 per request. Registration allowances and consent do not
grant user capabilities. Code/token issuance must independently compute live user
access and issuance ceilings under the existing authorization contract.

Requests accept `prompt=none`, `login`, `consent`, or `login consent`; absent prompt
uses the current session and remembered consent. `none` cannot be combined with
other values and never displays interaction: it returns `login_required`,
`consent_required`, or, when prepared but issuance is unavailable,
`temporarily_unavailable`. `max_age` accepts integer seconds from zero through 28800. Zero and `prompt=login` require a new session created after request start,
different from the session presented at the start. A bound transaction cannot be
transferred to a different session, even for the same user.

Consent is required on first use for every client, including administrator-created
clients. A current matching consent can cover an equal or narrower request;
expanded access, a different resource, or a changed client/application revision
requires consent again. Explicit `prompt=consent` requires a new approval for this
transaction. Approval replaces remembered scope consent for that principal/client/
resource; it does not union scopes or increase permissions. Approval and an
immutable consent audit record commit together. Denial terminates the transaction.

Malformed queries, duplicated recognized parameters, invalid encoding, unknown
clients and unregistered callbacks fail locally without redirecting. Only a
validated current callback can receive protocol errors. Other bounded unknown
parameters are ignored. A query is at most 8192 bytes and 32 parameters; malformed
or unsupported request syntax currently gets a local error even when a callback
could otherwise be valid. Broader protocol error interoperation remains part of
conformance qualification.

## Transaction and transport integrity

An OS-generated 256-bit browser handle lives only in a `Secure`, `HttpOnly`,
`SameSite=Lax`, host-only cookie, with path `/` and a five-minute maximum age.
Only its purpose-separated SHA-256 digest is stored. Neither handles nor protocol
credentials enter frontend storage. A separate public confirmation identifier
binds each consent submission to the request actually displayed: replacing the
cookie from another tab cannot redirect an old approval to a new request.

`/api/authorization` reads the current interaction. The decision endpoint accepts
only that confirmation identifier and `approve`/`deny`. The actor, client, callback,
challenge, nonce and requested access come from the cookie-bound stored request.
Unsafe methods require exact Origin, the CSRF header and same-origin Fetch Metadata
when supplied. Route admission is bounded and responses carry `no-store`,
`no-referrer`, frame protection and the existing CSP. There is no frontend server.

PostgreSQL is authoritative. Each begin/resume shares the security-state lock
before catalog/request/session locks, validates the current active client and
application, exact allowances, live credential epoch/session, request lifetime and
immutable binding. Revoked, expired or changed bindings fail closed; Redis cannot
make them valid. Requests started after a committed change observe it.

Requests expire after five minutes. Capacity is atomically limited to 100 retained
requests per client and 5000 overall, with indexed cleanup of at most 100 expired
rows per new request. A separate capacity row serializes creation; resumes lock
only their request after the shared authority lock. These conservative limits bound
memory and work but are not a throughput guarantee or abuse-rate limiter. Ingress
flood protection, production sizing and contention/load benchmarks remain release
work. Sustained quotas can deny new requests. Consent and audit retention also need
an operational policy; expiry cleanup does not erase audit records.

## Verification

`make test-provider` runs isolated policy/parser/crypto/operator tests. They use
source-defined inputs, including an explicitly public test-only RSA key, and do not
read files or require settings/services. `make test-postgres` exercises real
constraints, transaction races, expiry, revocation, consent rollback, quotas and
key lifecycle. `make test-browser` exercises the static consent portal through
verified HTTPS with real Rust, PostgreSQL and Redis. It imports the published JWK
using Node's crypto provider and independently checks its thumbprint; Rust unit
checks also verify an AWS-LC RS256 signature using ring.

The 2026-09-17 verification passed `make ci`, the final `make check`, 173 isolated
Rust/frontend/tooling tests, 31 PostgreSQL scenarios, five Redis infrastructure
scenarios and 14 limiter/login scenarios plus the separate-process helper. The
HTTPS browser run included operator generation, PKCS#8 import, duplicate import
rejection and retirement. Docker build and non-root/read-only smoke passed. A
clean staged export installed dependencies offline and passed all unit tests with
network access denied and no private input folders. Caddy 2.11.4 accepted the
updated development proxy configuration and its provider-to-Rust route mapping.

Core line/function/region coverage is 100%. Frontend unit coverage is 100% lines
and functions, 99.06% statements and 94.39% branches. Combined Rust unit,
PostgreSQL, Redis, operator and browser-driven process coverage is 98.40% lines,
99.56% functions and 92.56% regions, with 83 uncovered lines. Its 100% gate still
fails and remains unchanged. Missing paths include startup/terminal errors and
some provider parsing, persistence and transport failures. JavaScript tooling,
SQL triggers and production topology/load qualification remain separate.

Coverage qualification and full-provider interoperability remain open. The existing
100% authored-code target is unchanged. A successful pending request or independent
signature verification is not an OIDC conformance test. No external identity
application has completed a successful code exchange at this milestone.

## Protocol references

The choices and narrower project profile above use the
[OIDC authentication request](https://openid.net/specs/openid-connect-core-1_0.html#AuthRequest),
[discovery metadata](https://openid.net/specs/openid-connect-discovery-1_0.html#ProviderMetadata),
[PKCE S256](https://www.rfc-editor.org/rfc/rfc7636.html#section-4.3),
[resource indicators](https://www.rfc-editor.org/rfc/rfc8707.html#section-2.1),
[JWK thumbprints](https://www.rfc-editor.org/rfc/rfc7638.html#section-3.1), and
[back-channel Logout Tokens](https://openid.net/specs/openid-connect-backchannel-1_0.html#LogoutToken).
