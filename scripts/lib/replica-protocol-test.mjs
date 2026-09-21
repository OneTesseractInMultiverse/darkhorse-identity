import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";
import { validateCallback, validateIdToken } from "./reference-client.mjs";
export async function replicaProtocol({
  origin,
  requests,
  principal,
  password,
}) {
  let cookie = "",
    next = 0;
  const browser = async (path, body) => {
    const result = await requests[next++ % requests.length](path, {
      method: body === undefined ? "GET" : "POST",
      headers: {
        ...(cookie ? { cookie } : {}),
        origin,
        "x-darkhorse-csrf": "1",
        "content-type": "application/json",
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    for (const value of result.headers["set-cookie"] ?? []) {
      const pair = value.split(";")[0],
        name = pair.split("=")[0];
      cookie = [
        ...cookie.split("; ").filter((v) => v && !v.startsWith(`${name}=`)),
        pair,
      ].join("; ");
    }
    return {
      ...result,
      body: result.text.startsWith("{") ? JSON.parse(result.text) : null,
    };
  };
  assert.equal(
    (
      await browser("/api/auth/login", {
        email: "replicas@example.com",
        password,
      })
    ).status,
    200,
    "cross-replica login",
  );
  for (const request of requests)
    assert.equal(
      (await request("/api/auth/session", { headers: { cookie } })).status,
      200,
      "session exists on each replica",
    );
  const app = await browser("/api/admin/registration", {
    operation: "create_application",
    application: { name: "Replicated SSO", owner_id: principal, active: true },
  });
  assert.equal(app.status, 200);
  const client = await browser("/api/admin/registration", {
    operation: "create_client",
    application_id: app.body.record.id,
    client: {
      name: "Replicated client",
      active: true,
      refresh_tokens: true,
      redirect_uris: [`${origin}/callback`],
      resource_ids: [],
      scope_ids: [],
      token_endpoint_auth_method: "client_secret_basic",
    },
  });
  assert.equal(client.status, 200);
  const credentials = {
    client: client.body.record.id,
    secret: client.body.client_secret,
  };
  const authorization = `Basic ${Buffer.from(`${credentials.client}:${credentials.secret}`).toString("base64")}`;
  const form = (request, path, values) =>
    request(path, {
      method: "POST",
      headers: {
        authorization,
        "content-type": "application/x-www-form-urlencoded",
      },
      body: new URLSearchParams(values).toString(),
    });
  const introspect = (request, token) =>
    form(request, "/introspect", { token });
  const createCode = async () => {
    const verifier = randomBytes(32).toString("base64url");
    const expected = {
      issuer: origin,
      client: credentials.client,
      subject: principal,
      redirect: `${origin}/callback`,
      state: randomBytes(16).toString("hex"),
      nonce: randomBytes(24).toString("base64url"),
    };
    const query = new URLSearchParams({
      client_id: expected.client,
      redirect_uri: expected.redirect,
      response_type: "code",
      scope: "openid",
      prompt: "consent",
      state: expected.state,
      nonce: expected.nonce,
      code_challenge_method: "S256",
      code_challenge: createHash("sha256").update(verifier).digest("base64url"),
    });
    assert.equal((await browser(`/authorize?${query}`)).status, 303);
    const pending = (await browser("/api/authorization")).body;
    const decision = await browser("/api/authorization/decision", {
      request_id: pending.request_id,
      decision: "approve",
    });
    assert.equal(decision.status, 200);
    return {
      expected,
      values: {
        grant_type: "authorization_code",
        code: validateCallback(decision.body.redirect, expected),
        code_verifier: verifier,
        redirect_uri: expected.redirect,
      },
    };
  };
  const raced = await createCode();
  const codes = await Promise.all(
    requests.map((request) => form(request, "/token", raced.values)),
  );
  assert.deepEqual(
    codes.map((r) => r.status).sort(),
    [200, 400],
    "single-use code across replicas",
  );
  const codeWinner = JSON.parse(codes.find((r) => r.status === 200).text);
  for (const request of requests)
    assert.equal(
      JSON.parse((await introspect(request, codeWinner.access_token)).text)
        .active,
      false,
      "replayed code grant is revoked everywhere",
    );
  const mint = async () => {
    const value = await createCode(),
      response = await form(requests[0], "/token", value.values);
    assert.equal(response.status, 200);
    const tokens = JSON.parse(response.text),
      keys = JSON.parse((await requests[1]("/jwks")).text).keys;
    validateIdToken(tokens.id_token, keys, {
      ...value.expected,
      now: Math.floor(Date.now() / 1000),
    });
    return tokens;
  };
  const original = await mint();
  const rotations = await Promise.all(
    requests.map((request) =>
      form(request, "/token", {
        grant_type: "refresh_token",
        refresh_token: original.refresh_token,
      }),
    ),
  );
  assert.deepEqual(
    rotations.map((r) => r.status).sort(),
    [200, 400],
    "refresh has one winner across replicas",
  );
  const refreshWinner = JSON.parse(
    rotations.find((r) => r.status === 200).text,
  );
  for (const request of requests)
    assert.equal(
      JSON.parse((await introspect(request, refreshWinner.access_token)).text)
        .active,
      false,
      "refresh replay revokes family everywhere",
    );
  const live = await mint();
  const check = async (expected) => {
    for (const request of requests) {
      const r = await introspect(request, live.access_token);
      assert.equal(r.status, 200);
      assert.equal(JSON.parse(r.text).active, expected);
    }
  };
  await check(true);
  console.log(
    "Two replicas: shared browser session, cross-pod consent, independent RS256, single-winner code/refresh and replay revocation passed.",
  );
  return {
    check,
    introspect: (request) => introspect(request, live.access_token),
    browser,
    credentials,
  };
}
export async function sharedBudgets(origin, requests) {
  const statuses = [];
  for (let n = 0; n < 6; n++)
    statuses.push(
      (
        await requests[n % requests.length]("/api/auth/login", {
          method: "POST",
          headers: {
            origin,
            "x-darkhorse-csrf": "1",
            "content-type": "application/json",
          },
          body: JSON.stringify({
            email: "absent@example.com",
            password: "not a real password for any account",
          }),
        })
      ).status,
    );
  assert.deepEqual(
    statuses,
    [401, 401, 401, 401, 401, 429],
    "independent replicas share one account budget",
  );
}
