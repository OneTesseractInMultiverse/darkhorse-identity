# API reference

Darkhorse publishes a versioned OpenAPI 3.2.1 document for the supported
confidential-client OpenID Connect endpoints. The management console's **API
documentation** page reads the same-build static JSON, provides local search,
operation details, copyable request examples and stable operation fragments, and
offers the source specification for download. The Rust server serves the
prebuilt files at `/reference/openapi-v1.json` and
`/reference/route-classification-v1.json`. No documentation service or remote
schema resolver is used.

The reference page is read-only. It does not submit protocol requests, use the
browser session to call APIs, show live client registrations, or contain
deployment secrets. A page being visible does not enable an endpoint or prove
that a deployment has its issuer, signing key, client, or resource configuration
ready.

## Supported integration operations

[`openapi-v1.json`](api/openapi-v1.json) currently documents these seven
third-party integration routes:

| Method | Path                                | Operation                                                |
| ------ | ----------------------------------- | -------------------------------------------------------- |
| GET    | `/.well-known/openid-configuration` | Read provider metadata                                   |
| GET    | `/jwks`                             | Read public signing keys                                 |
| GET    | `/authorize`                        | Start Authorization Code with PKCE                       |
| POST   | `/token`                            | Redeem a code or rotate an opted-in refresh token        |
| GET    | `/userinfo`                         | Read scope-limited identity claims                       |
| POST   | `/introspect`                       | Check an opaque credential as an authorized caller       |
| POST   | `/revoke`                           | Revoke a credential in the caller's own issuance context |

Access and refresh tokens are opaque; identity ID tokens are signed RS256 JWTs.
`client_secret_basic` is the confidential-client authentication method. The
reference records request media types, parameters, response shapes, validation
bounds, errors, cache behavior, and security notes for this implemented
protocol slice. It does not describe browser logout, back-channel logout-token
delivery, recovery, or an unimplemented feature as an available endpoint.

OAuth request parsing follows the protocol's extension rule: bounded parameters
that Darkhorse does not recognize are ignored, while duplicate recognized
parameters and the supported endpoints' explicitly forbidden body-authentication
fields are rejected. This preserves interoperability for OAuth extensions
without accepting client credentials in a form body. See [RFC 6749 §3.2](https://www.rfc-editor.org/rfc/rfc6749.html#section-3.2).

The OpenAPI schemas are a reviewed publication of the transport contract rather
than Rust-generated request or response types. The check verifies their release
version, local references, and exact operation parity with the source-derived
route classification. Transport behavior is independently covered by Rust
tests. A schema edit still requires review against the HTTP handlers and those
tests; route parity alone does not prove that every schema constraint matches
runtime behavior. This is an identified qualification item for issue #43.

## Route boundary inventory

[`route-registration-v1.json`](api/route-registration-v1.json) is generated from
the Rust adapter/server source without starting the application.
[`route-classification-v1.json`](api/route-classification-v1.json) assigns every
discovered registration a reviewed caller surface. Only entries carrying an
`operation_id` are declared supported third-party protocol operations. The
other groups identify first-party browser/management endpoints, public sign-in
presentation data, operational health probes, and static files. They remain
subject to the runtime authorization checks and are not API grants or a promise
of integration stability.

The source walk is bounded to 1,024 Rust files, 4,096 directory entries, 32
directory levels, 1 MiB per source file, 32 MiB total source and 512 route
registrations. It fails closed on unsupported router syntax and symlinks. This
is a static syntax inventory: it does not execute configuration branches or
prove route availability, authentication, authorization, request/response
schemas, or feature prerequisites. Do not generate clients or access policy
from the inventory.

## Credential boundaries

- Darkhorse browser pages use the host-only HTTP-only session cookie. Browser
  mutation APIs require their same-origin and CSRF checks. An application must
  never copy that cookie to its own backend.
- A confidential OIDC client uses its registered client ID and secret with
  `client_secret_basic` from its backend over verified HTTPS. An application ID
  identifies an application record; it is not a client ID.
- An opaque identity access token is sent as a bearer credential to UserInfo.
  It is not the signed ID token, a refresh token, or an introspection caller
  credential.
- A resource server authenticates to introspection with its own resource-server
  identifier and secret. It must deny access when introspection is inactive,
  rate-limited, or unavailable and must not reuse a cached positive decision.
- A personal API key represents a user's explicitly limited resource and
  capability grants. It is distinct from both client and resource-server
  secrets.

Use verified HTTPS throughout. Keep credentials out of URLs, browser storage,
command arguments, and logs. Read each operation's documented failure and
uncertain-outcome behavior before retrying a write or credential exchange.

The console now includes a step-by-step confidential web-client flow and
separates identity UserInfo from protected-resource authorization and
introspection. It is a protocol outline rather than a runnable client sample.
For implementation and operations detail, consult the
[provider flow](provider.md), [registration](registration.md),
[token checks](token-checks.md), [resource introspection](resource-introspection.md),
[personal API keys](personal-api-keys.md), [console](console.md), and the
feature-specific guides. Runnable end-to-end examples and deployment
qualification remain in progress under issue #43.

## Generate and verify

After changing an Axum registration, regenerate and review both source-derived
snapshots:

```sh
make api-inventory-generate
make api-classification-generate
git diff -- docs/api/route-registration-v1.json docs/api/route-classification-v1.json
```

After reviewing an OpenAPI contract change, update the static bundle and run the
focused checks:

```sh
make api-reference-bundle
make api-inventory-check
make api-classification-check
make api-spec-check
make check
make build-web
```

`api-reference-bundle` copies the reviewed specification and classification
source byte-for-byte into the static frontend. `api-spec-check` rejects release
version drift, missing/extra/renamed integration operations, duplicate operation
IDs, unresolved or remote `$ref` values, invalid inputs, and stale static copies.
The checks use only checked-in files and local tools; they do not start a service
or fetch schemas. The specification format follows the
[OpenAPI 3.2.1 specification](https://spec.openapis.org/oas/v3.2.1.html).

`make check` includes the deterministic route and contract drift checks. The
API reference page is a route-specific lazy-loaded frontend feature; unrelated
console pages do not fetch its JSON documents. The disposable HTTPS browser suite
checks the documented flow, search, direct links and reloads, keyboard
disclosure, clipboard and download actions, safe text rendering, Spanish
presentation, signed-out behavior, cookie omission on static-reference fetches,
and mobile/zoom-sized viewports. Runnable application examples, screen-reader
review, repeatable bundle/load performance budgets, and mechanical
runtime-schema parity remain tracked work for issue #43.
