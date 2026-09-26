import {
  ClientSecretBasic,
  authorizationCodeGrant,
  buildAuthorizationUrl,
  calculatePKCECodeChallenge,
  discovery,
  fetchUserInfo,
  randomNonce,
  randomPKCECodeVerifier,
  randomState,
  customFetch,
} from "openid-client";

const clients = new WeakMap();

function secureUrl(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    throw new TypeError(`${label} must be a valid HTTPS URL.`);
  }
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new TypeError(`${label} must be a valid HTTPS URL.`);
  }
  if (url.protocol !== "https:" || !url.hostname)
    throw new TypeError(`${label} must use HTTPS.`);
  if (url.username || url.password)
    throw new TypeError(`${label} must not contain credentials.`);
  if (url.search)
    throw new TypeError(`${label} must not contain a query string.`);
  if (url.hash) throw new TypeError(`${label} must not contain a fragment.`);
  return url;
}

function clientSettings(client) {
  const settings = clients.get(client);
  if (!settings) throw new TypeError("OIDC client configuration is invalid.");
  return settings;
}

function transactionValues(transaction) {
  if (
    !transaction ||
    typeof transaction.state !== "string" ||
    transaction.state.length === 0 ||
    typeof transaction.nonce !== "string" ||
    transaction.nonce.length === 0 ||
    typeof transaction.codeVerifier !== "string" ||
    transaction.codeVerifier.length === 0
  ) {
    throw new TypeError("OIDC authorization transaction is incomplete.");
  }
  return transaction;
}

function isResourceScope(scope) {
  return (
    typeof scope === "string" &&
    scope.length > 0 &&
    scope.length <= 100 &&
    /^[\x21\x23-\x5b\x5d-\x7e]+$/.test(scope) &&
    ![
      "openid",
      "profile",
      "email",
      "address",
      "phone",
      "offline_access",
    ].includes(scope)
  );
}

function isResourceAuthorizationRequest(audience, scopes) {
  return (
    typeof audience === "string" &&
    /^urn:darkhorse:resource:[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(
      audience,
    ) &&
    Array.isArray(scopes) &&
    scopes.length >= 1 &&
    scopes.length <= 32 &&
    scopes.every(isResourceScope) &&
    new Set(scopes).size === scopes.length
  );
}

async function beginAuthorizationWithScope(client, scope, resource) {
  const { configuration, redirectUri } = clientSettings(client);
  const codeVerifier = randomPKCECodeVerifier();
  const codeChallenge = await calculatePKCECodeChallenge(codeVerifier);
  const transaction = Object.freeze({
    state: randomState(),
    nonce: randomNonce(),
    codeVerifier,
  });
  const parameters = {
    redirect_uri: redirectUri.href,
    response_type: "code",
    scope,
    state: transaction.state,
    nonce: transaction.nonce,
    code_challenge: codeChallenge,
    code_challenge_method: "S256",
  };
  if (resource !== undefined) parameters.resource = resource;
  const authorizationUrl = buildAuthorizationUrl(configuration, parameters);
  return Object.freeze({ authorizationUrl, transaction });
}

/**
 * Discover a Darkhorse issuer and configure the confidential client using
 * client_secret_basic. The secret remains inside the OIDC library config.
 */
export async function createConfidentialOidcClient({
  issuer,
  clientId,
  clientSecret,
  redirectUri,
  fetch = globalThis.fetch,
}) {
  const issuerUrl = secureUrl(issuer, "OIDC issuer");
  const callbackUrl = secureUrl(redirectUri, "OIDC callback");
  if (
    typeof clientId !== "string" ||
    clientId.length === 0 ||
    /[\u0000-\u001f\u007f]/.test(clientId) ||
    typeof clientSecret !== "string" ||
    clientSecret.length === 0 ||
    /[\u0000-\u001f\u007f]/.test(clientSecret)
  ) {
    throw new TypeError("OIDC client credentials are invalid.");
  }
  if (typeof fetch !== "function")
    throw new TypeError("OIDC fetch must be a function.");

  const configuration = await discovery(
    issuerUrl,
    clientId,
    {
      redirect_uris: [callbackUrl.href],
      token_endpoint_auth_method: "client_secret_basic",
    },
    ClientSecretBasic(clientSecret),
    { [customFetch]: fetch },
  );
  const client = Object.freeze({});
  clients.set(client, {
    configuration,
    issuerValue: issuer,
    redirectUri: callbackUrl,
  });
  return client;
}

/** Generate a one-time authorization URL and server-side transaction values. */
export async function beginAuthorization(client) {
  return beginAuthorizationWithScope(client, "openid");
}

/** Begin an Authorization Code flow for one resource and its registered scopes. */
export async function beginResourceAuthorization(client, request = {}) {
  const audience = request?.audience;
  const scopes = request?.scopes;
  if (!isResourceAuthorizationRequest(audience, scopes)) {
    throw new TypeError("resource authorization parameters are invalid.");
  }
  return beginAuthorizationWithScope(
    client,
    ["openid", ...scopes].join(" "),
    audience,
  );
}

/**
 * Validate and redeem one callback. Call only from the backend, after loading
 * and consuming the transaction from its server-side session store.
 */
export async function completeAuthorization(client, callback, transaction) {
  const { configuration, issuerValue, redirectUri } = clientSettings(client);
  transactionValues(transaction);
  let callbackUrl;
  try {
    callbackUrl = callback instanceof URL ? callback : new URL(callback);
  } catch {
    throw new TypeError("OIDC callback URL is invalid.");
  }
  if (
    callbackUrl.protocol !== "https:" ||
    callbackUrl.username ||
    callbackUrl.password ||
    callbackUrl.hash ||
    callbackUrl.origin !== redirectUri.origin ||
    callbackUrl.pathname !== redirectUri.pathname
  ) {
    throw new TypeError(
      "OIDC callback URL does not match the registered redirect URI.",
    );
  }
  const responseIssuer = callbackUrl.searchParams.get("iss");
  if (responseIssuer !== null && responseIssuer !== issuerValue) {
    throw new TypeError(
      "OIDC authorization response issuer does not match discovery.",
    );
  }
  return authorizationCodeGrant(configuration, callbackUrl, {
    expectedState: transaction.state,
    expectedNonce: transaction.nonce,
    pkceCodeVerifier: transaction.codeVerifier,
  });
}

/** Fetch UserInfo on the backend and bind its subject to the validated ID token. */
export async function loadUserInfo(client, accessToken, expectedSubject) {
  const { configuration } = clientSettings(client);
  if (
    typeof accessToken !== "string" ||
    accessToken.length === 0 ||
    typeof expectedSubject !== "string" ||
    expectedSubject.length === 0
  ) {
    throw new TypeError("OIDC UserInfo credentials are invalid.");
  }
  return fetchUserInfo(configuration, accessToken, expectedSubject);
}
