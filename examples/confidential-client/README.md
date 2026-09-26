# Confidential Node.js client example

This framework-neutral Node.js example shows the server-side parts of a
confidential OpenID Connect client using the maintained `openid-client`
library. It discovers the issuer, uses `client_secret_basic`, creates an
Authorization Code request with S256 PKCE, binds the callback to state and
nonce, validates the signed ID token, and fetches UserInfo with the opaque
identity access token.

The source is in `src/oidc-client.mjs`. Import it only from your application
backend. Never bundle it into browser code. Keep the client secret and Darkhorse
tokens in a server-side secret/session store. Store each returned transaction
(`state`, `nonce`, and `codeVerifier`) in the initiating server-side session,
consume it once when handling the callback, and never place it in a cookie,
URL, browser storage, command line, or log. Use your framework's maintained
session and cookie facilities to issue your own `Secure`, `HttpOnly` session
cookie after successful validation.

The example deliberately leaves HTTP routing, application sessions, and secret
manager integration to the host application. Those choices depend on its web
framework and deployment. The returned token response is sensitive and must
remain on the backend. Do not retry code exchange after an uncertain response;
start a new authorization flow instead.

## Protect a resource API

`src/oidc-client.mjs` also exports `beginResourceAuthorization`. Pass one
registered resource audience and only its registered resource scopes. The helper
adds `openid`, PKCE, state and nonce. The user must approve the resource grant;
the example does not ask for identity claims such as `profile` or `email` in the
same request. The resulting signed ID token authenticates the user to the OIDC
client. Its opaque access token is intended for the named API, not UserInfo.

The protected API then calls `requestResourceIntrospection` from
`src/resource-access.mjs` using the access token and that resource's dedicated
`rs_<resource UUID>` ID and one-time secret. These HTTP Basic credentials differ
from the OIDC client's `client_id` and `client_secret`: they authenticate the API
to `/introspect`, not the user's authorization request or token exchange. Keep
both credential pairs in backend-only secret storage. Do not send them to the
browser, put them in URLs or command arguments, or log them.

Example backend authorization check:

```js
import {
  hasResourceCapability,
  requestResourceIntrospection,
} from "./resource-access.mjs";

async function authorizeProtectedRequest(requestBearerToken) {
  const decision = await requestResourceIntrospection({
    issuer: "https://identity.example",
    introspectionClientId: resourceConfig.introspectionClientId,
    introspectionSecret: resourceConfig.introspectionSecret,
    accessToken: requestBearerToken,
  });

  return hasResourceCapability(decision, {
    issuer: "https://identity.example",
    audience: "urn:darkhorse:resource:<resource UUID>",
    capability: "<required capability UUID>",
    nowSeconds: Math.floor(Date.now() / 1000),
  });
}
```

The check requires a fresh active response with the exact issuer, audience, live
expiry and requested capability. Transport failure, an unavailable response,
malformed JSON, inactive credentials or any mismatch denies access. The helper
limits response bodies to 16 KiB. It does not retry, cache positive decisions or
fall back to local role/scope guesses; perform it for each protected request.
Apply object-level authorization in the
resource API as well. Keep the protected write separate from the introspection
call and account for the small time window between a successful check and that
write.

The disposable HTTPS browser suite runs this complete example against the local
server. It checks the fresh resource-consent flow, callback/state validation,
ID-token audience and nonce, rejection of the resource token at UserInfo, and the
separate resource-server credentials and capability check.

## Run the tests

From the repository root, install the locked dependencies and run the isolated
tests:

```sh
make deps-install
make test-example
```

The disposable HTTPS browser suite then executes the same example against a
temporary Darkhorse server and database, including incorrect-state and
incorrect-PKCE failures, a successful token exchange, ID-token validation, and
UserInfo:

```sh
make browser-install
make test-browser
```

The browser suite uses synthetic users and credentials and cleans up the
containers it creates. Its test-only TLS adapter trusts only the generated test
CA. For a deployed issuer, keep normal certificate validation enabled and trust
the issuer certificate through the host's approved CA configuration.
