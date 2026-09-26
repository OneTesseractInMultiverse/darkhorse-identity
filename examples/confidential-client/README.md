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
