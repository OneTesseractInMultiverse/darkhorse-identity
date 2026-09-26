import assert from "node:assert/strict";
import { test } from "node:test";
import { createHash } from "node:crypto";
import {
  beginAuthorization,
  completeAuthorization,
  createConfidentialOidcClient,
  loadUserInfo,
} from "../../src/oidc-client.mjs";

const issuer = "https://identity.example.test";
const redirectUri = "https://application.example.test/oidc/callback";
const clientId = "synthetic-client-id";
const clientSecret = "synthetic-client-secret";

function createDiscoveryFetch(overrides = {}) {
  const requests = [];
  const metadata = {
    issuer,
    authorization_endpoint: `${issuer}/authorize`,
    token_endpoint: `${issuer}/token`,
    jwks_uri: `${issuer}/jwks`,
    userinfo_endpoint: `${issuer}/userinfo`,
    response_types_supported: ["code"],
    subject_types_supported: ["public"],
    id_token_signing_alg_values_supported: ["RS256"],
    token_endpoint_auth_methods_supported: ["client_secret_basic"],
    ...overrides,
  };
  return {
    requests,
    fetch: async (input, init) => {
      requests.push({ url: String(input), method: init?.method ?? "GET" });
      return Response.json(metadata);
    },
  };
}

function createClient(fetch) {
  return createConfidentialOidcClient({
    issuer,
    clientId,
    clientSecret,
    redirectUri,
    fetch,
  });
}

test("client setup rejects non-HTTPS issuer and callback URLs", async () => {
  const mock = createDiscoveryFetch();
  await assert.rejects(
    createConfidentialOidcClient({
      issuer: "http://identity.example.test",
      clientId,
      clientSecret,
      redirectUri,
      fetch: mock.fetch,
    }),
    /OIDC issuer must use HTTPS/,
  );
  await assert.rejects(
    createConfidentialOidcClient({
      issuer,
      clientId,
      clientSecret,
      redirectUri: "http://application.example.test/oidc/callback",
      fetch: mock.fetch,
    }),
    /OIDC callback must use HTTPS/,
  );
  assert.deepEqual(mock.requests, []);
});

test("client setup rejects credentials in URLs and callback query parameters", async () => {
  const mock = createDiscoveryFetch();
  await assert.rejects(
    createConfidentialOidcClient({
      issuer: "https://user:password@identity.example.test",
      clientId,
      clientSecret,
      redirectUri,
      fetch: mock.fetch,
    }),
    /OIDC issuer must not contain credentials/,
  );
  await assert.rejects(
    createConfidentialOidcClient({
      issuer,
      clientId,
      clientSecret,
      redirectUri: `${redirectUri}?campaign=1`,
      fetch: mock.fetch,
    }),
    /OIDC callback must not contain a query string/,
  );
  assert.deepEqual(mock.requests, []);
});

test("client setup rejects malformed URLs, unsafe credentials and invalid fetch adapters", async () => {
  const mock = createDiscoveryFetch();
  const invalidSetups = [
    { issuer: "", message: /valid HTTPS URL/ },
    { issuer: "not an absolute URL", message: /valid HTTPS URL/ },
    { issuer, redirectUri: `${redirectUri}#fragment`, message: /fragment/ },
    { issuer, clientId: "", message: /client credentials are invalid/ },
    {
      issuer,
      clientId: "bad\nclient",
      message: /client credentials are invalid/,
    },
    { issuer, clientSecret: "", message: /client credentials are invalid/ },
    {
      issuer,
      clientSecret: "bad\u007fsecret",
      message: /client credentials are invalid/,
    },
    { issuer, fetch: null, message: /fetch must be a function/ },
  ];

  for (const { message, ...overrides } of invalidSetups) {
    await assert.rejects(
      createConfidentialOidcClient({
        issuer,
        clientId,
        clientSecret,
        redirectUri,
        fetch: mock.fetch,
        ...overrides,
      }),
      message,
    );
  }
  assert.deepEqual(mock.requests, []);
});

test("authorization creates distinct state, nonce and S256 PKCE transactions", async () => {
  const mock = createDiscoveryFetch();
  const client = await createClient(mock.fetch);
  const first = await beginAuthorization(client);
  const second = await beginAuthorization(client);
  const parameters = first.authorizationUrl.searchParams;

  assert.equal(first.authorizationUrl.origin, issuer);
  assert.equal(first.authorizationUrl.pathname, "/authorize");
  assert.equal(parameters.get("client_id"), clientId);
  assert.equal(parameters.get("redirect_uri"), redirectUri);
  assert.equal(parameters.get("response_type"), "code");
  assert.equal(parameters.get("scope"), "openid");
  assert.equal(parameters.get("state"), first.transaction.state);
  assert.equal(parameters.get("nonce"), first.transaction.nonce);
  assert.equal(parameters.get("code_challenge_method"), "S256");
  assert.equal(
    parameters.get("code_challenge"),
    createHash("sha256")
      .update(first.transaction.codeVerifier)
      .digest("base64url"),
  );
  assert.notEqual(first.transaction.state, second.transaction.state);
  assert.notEqual(first.transaction.nonce, second.transaction.nonce);
  assert.notEqual(
    first.transaction.codeVerifier,
    second.transaction.codeVerifier,
  );
  assert.equal(first.authorizationUrl.searchParams.has("client_secret"), false);
});

test("client operations reject handles that were not created by discovery", async () => {
  await assert.rejects(beginAuthorization({}), /configuration is invalid/);
  await assert.rejects(
    completeAuthorization({}, "https://application.example.test/callback", {}),
    /configuration is invalid/,
  );
  await assert.rejects(
    loadUserInfo({}, "opaque-token", "subject"),
    /configuration is invalid/,
  );
});

test("callback processing rejects an unrelated callback before token exchange", async () => {
  const mock = createDiscoveryFetch();
  const client = await createClient(mock.fetch);
  const transaction = (await beginAuthorization(client)).transaction;
  const requestCountAfterDiscovery = mock.requests.length;

  await assert.rejects(
    completeAuthorization(
      client,
      new URL(
        "https://attacker.example.test/oidc/callback?code=synthetic&state=synthetic",
      ),
      transaction,
    ),
    /callback URL does not match the registered redirect URI/,
  );
  assert.equal(mock.requests.length, requestCountAfterDiscovery);

  await assert.rejects(
    completeAuthorization(client, "not a URL", transaction),
    /callback URL is invalid/,
  );
  await assert.rejects(
    completeAuthorization(
      client,
      `http://application.example.test/oidc/callback?code=synthetic`,
      transaction,
    ),
    /callback URL does not match the registered redirect URI/,
  );
  await assert.rejects(
    completeAuthorization(
      client,
      `${redirectUri}/other?code=synthetic`,
      transaction,
    ),
    /callback URL does not match the registered redirect URI/,
  );
  await assert.rejects(
    completeAuthorization(client, `${redirectUri}#fragment`, transaction),
    /callback URL does not match the registered redirect URI/,
  );
});

test("callback processing requires all transaction checks and rejects changed state before exchange", async () => {
  const mock = createDiscoveryFetch();
  const client = await createClient(mock.fetch);
  const transaction = (await beginAuthorization(client)).transaction;
  const callback = new URL(redirectUri);
  callback.searchParams.set("code", "synthetic-authorization-code");
  callback.searchParams.set("state", "changed-state");
  callback.searchParams.set("iss", issuer);
  const requestCountAfterDiscovery = mock.requests.length;

  await assert.rejects(
    completeAuthorization(client, callback, {
      state: transaction.state,
      nonce: transaction.nonce,
      codeVerifier: transaction.codeVerifier,
    }),
    /invalid response/,
  );
  assert.equal(mock.requests.length, requestCountAfterDiscovery);
  await assert.rejects(
    completeAuthorization(client, callback, {
      state: "",
      nonce: transaction.nonce,
      codeVerifier: transaction.codeVerifier,
    }),
    /authorization transaction is incomplete/,
  );

  const wrongIssuer = new URL(redirectUri);
  wrongIssuer.searchParams.set("code", "synthetic-authorization-code");
  wrongIssuer.searchParams.set("state", transaction.state);
  wrongIssuer.searchParams.set("iss", "https://other.example.test");
  await assert.rejects(
    completeAuthorization(client, wrongIssuer, transaction),
    /issuer does not match discovery/,
  );
  assert.equal(mock.requests.length, requestCountAfterDiscovery);
});

test("UserInfo stays server-side, uses a bearer token and checks the expected subject", async () => {
  const requests = [];
  const metadata = {
    issuer,
    authorization_endpoint: `${issuer}/authorize`,
    token_endpoint: `${issuer}/token`,
    jwks_uri: `${issuer}/jwks`,
    userinfo_endpoint: `${issuer}/userinfo`,
    response_types_supported: ["code"],
    subject_types_supported: ["public"],
    id_token_signing_alg_values_supported: ["RS256"],
    token_endpoint_auth_methods_supported: ["client_secret_basic"],
  };
  const client = await createClient(async (input, init) => {
    const url = new URL(String(input));
    requests.push({ url, headers: new Headers(init?.headers) });
    return url.pathname.endsWith("openid-configuration")
      ? Response.json(metadata)
      : Response.json({ sub: "subject-123", name: "Example User" });
  });

  assert.deepEqual(
    await loadUserInfo(client, "opaque-access-token", "subject-123"),
    { sub: "subject-123", name: "Example User" },
  );
  assert.equal(requests.at(-1).url.pathname, "/userinfo");
  assert.equal(
    requests.at(-1).headers.get("authorization"),
    "Bearer opaque-access-token",
  );
  await assert.rejects(
    loadUserInfo(client, "", "subject-123"),
    /UserInfo credentials are invalid/,
  );
  await assert.rejects(
    loadUserInfo(client, "opaque-access-token", ""),
    /UserInfo credentials are invalid/,
  );
});
