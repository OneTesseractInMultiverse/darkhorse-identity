// Qualification against the running Rust provider, with no signing-state SQL.
import assert from "node:assert/strict";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { setTimeout as delay } from "node:timers/promises";
import { discoverJwks } from "./reference-discovery.mjs";
import { createJwksTransport } from "./reference-jwks-https.mjs";
import { parseJwks } from "./reference-jwks.mjs";
import {
  exchange,
  idTokenKeyId,
  validateCallback,
  validateIdToken,
} from "./reference-client.mjs";
import { activatePublishedKey } from "./logout-jwks-signing.mjs";
import { closeBrowserResources } from "./browser-cleanup.mjs";

async function api(
  page,
  path,
  body,
  method = body === undefined ? "GET" : "POST",
) {
  return page.evaluate(
    async ({ path, body, method }) => {
      const response = await fetch(path, {
        method,
        cache: "no-store",
        headers: {
          "content-type": "application/json",
          "x-darkhorse-csrf": "1",
        },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
      const text = await response.text();
      return { status: response.status, body: text ? JSON.parse(text) : null };
    },
    { path, body, method },
  );
}
async function login(page, origin, email, password) {
  await page.goto(origin);
  assert.equal(
    (await api(page, "/api/auth/login", { email, password })).status,
    200,
  );
  await page.goto(origin);
}
async function register(page, origin, principal, receiver, name) {
  const application = await api(page, "/api/admin/registration", {
    operation: "create_application",
    application: { name, owner_id: principal, active: true },
  });
  assert.equal(application.status, 200);
  const redirect = `${origin}/jwks-logout-${name}`;
  const registered = await api(page, "/api/admin/registration", {
    operation: "create_client",
    application_id: application.body.record.id,
    client: {
      name,
      active: true,
      redirect_uris: [redirect],
      resource_ids: [],
      scope_ids: [],
      token_endpoint_auth_method: "client_secret_basic",
    },
  });
  assert.equal(registered.status, 200);
  const client = registered.body.record.id;
  const path = `/api/admin/applications/${application.body.record.id}/clients/${client}/logout`;
  const current = await api(page, path);
  assert.equal(current.status, 200);
  const configured = await api(
    page,
    path,
    {
      revision: current.body.configuration.revision,
      backchannel_logout_uri: `${receiver.origin}/${client}`,
      backchannel_logout_session_required: true,
    },
    "PUT",
  );
  assert.equal(configured.status, 200);
  return { client, secret: registered.body.client_secret, redirect };
}
async function authorize(page, origin, ca, client, subject) {
  const verifier = randomBytes(32).toString("base64url");
  const expected = {
    issuer: origin,
    client: client.client,
    subject,
    redirect: client.redirect,
    state: randomBytes(32).toString("base64url"),
    nonce: randomBytes(32).toString("base64url"),
  };
  const query = new URLSearchParams({
    client_id: client.client,
    redirect_uri: client.redirect,
    response_type: "code",
    scope: "openid",
    prompt: "consent",
    ui_locales: "en",
    state: expected.state,
    nonce: expected.nonce,
    code_challenge_method: "S256",
    code_challenge: createHash("sha256").update(verifier).digest("base64url"),
  });
  await page.goto(`${origin}/authorize?${query}`);
  const reference = new URL(page.url()).searchParams.get("request");
  assert.match(reference, /^[a-f0-9]{64}$/);
  const pending = await api(page, `/api/authorization?request=${reference}`);
  assert.equal(pending.status, 200);
  assert.equal(pending.body.status, "consent");
  const approved = await api(
    page,
    `/api/authorization/decision?request=${reference}`,
    { request_id: reference, decision: "approve" },
  );
  assert.equal(approved.status, 200);
  const code = validateCallback(approved.body.redirect, expected);
  const result = await exchange(
    origin,
    ca,
    client.client,
    client.secret,
    client.redirect,
    code,
    verifier,
  );
  assert.equal(result.status, 200);
  await page.goto(origin);
  return { token: result.body.id_token, expected };
}
async function bind(receiver, id, client, authorization) {
  return receiver.bind(
    id,
    client.client,
    authorization.token,
    authorization.expected.subject,
    authorization.expected.nonce,
  );
}
async function waitForSessions(receiver, expected, signal) {
  for (let attempt = 0; attempt < 300; attempt++) {
    if (JSON.stringify(receiver.sessions()) === JSON.stringify(expected))
      return;
    await delay(100, undefined, { signal });
  }
  assert.deepEqual(
    receiver.sessions(),
    expected,
    "Rust logout delivery must finish within the 30-second scenario budget",
  );
}
export async function verifyDeployedJwks({
  browser,
  page,
  origin,
  ca,
  password,
  principal,
  invoke,
  runSql,
  receiver,
  signal,
}) {
  const trusted = await discoverJwks({
    issuer: origin,
    url: `${origin}/jwks`,
    ca,
  });
  const first = await register(page, origin, principal, receiver, "JWKS-A");
  const second = await register(page, origin, principal, receiver, "JWKS-B");
  receiver.configure(trusted, [first.client, second.client]);
  const oldA = await authorize(page, origin, ca, first, principal);
  const oldB = await authorize(page, origin, ca, second, principal);
  const a = await bind(receiver, "old-a", first, oldA);
  const b = await bind(receiver, "old-b", second, oldB);
  assert.equal(a.bound, true);
  assert.equal(b.bound, true);
  assert.notEqual(
    a.claims.sid,
    b.claims.sid,
    "each audience has its own signed session reference",
  );
  assert.equal((await bind(receiver, "old-tab", first, oldA)).bound, true);
  await assert.rejects(bind(receiver, "wrong-client", second, oldA));
  const oldKid = idTokenKeyId(oldA.token, {
    ...oldA.expected,
    now: Math.floor(Date.now() / 1000),
  });
  assert.deepEqual(receiver.evidence(), {
    fetches: 1,
    kids: [oldKid],
    deliveries: [],
  });

  const inventory = JSON.parse((await invoke(["signing-status"])).stdout);
  const staged = JSON.parse(
    (await invoke(["signing-generate", String(inventory.revision)])).stdout,
  );
  const publishedAt = performance.now();
  assert.notEqual(staged.kid, oldKid);
  const readPublished = async () =>
    parseJwks(await createJwksTransport()(trusted));
  const published = await readPublished();
  assert.ok(published.some((key) => key.kid === oldKid));
  assert.ok(
    published.some((key) => key.kid === staged.kid),
    "the real /jwks route publishes a staged key before activation",
  );
  assert.equal(
    receiver.evidence().fetches,
    1,
    "publication cannot inject keys into the receiver",
  );

  let newerContext, otherContext, failure;
  try {
    newerContext = await browser.newContext({ locale: "en-US" });
    otherContext = await browser.newContext({ locale: "en-US" });
    const newerPage = await newerContext.newPage(),
      otherPage = await otherContext.newPage();
    await login(newerPage, origin, "browser@example.com", password);
    const newer = await authorize(newerPage, origin, ca, first, principal);
    const newBinding = await bind(receiver, "newer", first, newer);
    assert.equal(newBinding.bound, true);
    assert.notEqual(newBinding.claims.sid, a.claims.sid);
    // Seed only an unrelated fixture identity, then use normal login/consent/code
    // exchange. No provider keys, lifecycle times, RP bindings or tokens are seeded.
    const other = randomUUID(),
      credential = randomUUID();
    assert.match(principal, /^[a-f0-9-]{36}$/);
    await runSql(`INSERT INTO principals(id,email,first_name,last_name) VALUES('${other}','jwks-unrelated@example.com','JWKS','Unrelated');
INSERT INTO credentials(id,principal_id,kind) VALUES('${credential}','${other}','password');
INSERT INTO password_credentials(credential_id,verifier) SELECT '${credential}',pc.verifier FROM password_credentials pc JOIN credentials c ON c.id=pc.credential_id WHERE c.principal_id='${principal}' AND NOT c.revoked;`);
    await login(otherPage, origin, "jwks-unrelated@example.com", password);
    const unrelated = await authorize(otherPage, origin, ca, first, other);
    assert.equal(
      (await bind(receiver, "unrelated", first, unrelated)).bound,
      true,
    );

    await activatePublishedKey(invoke, staged, { publishedAt, signal });
    const active = JSON.parse((await invoke(["signing-status"])).stdout);
    const replacement = active.keys.find((key) => key.kid === staged.kid);
    const overlap = active.keys.find((key) => key.kid === oldKid);
    assert.equal(replacement.phase, "active");
    assert.equal(overlap.phase, "retiring");
    assert.equal(overlap.verify_until_ms - replacement.activated_ms, 600000);
    const keys = await readPublished();
    assert.ok(keys.some((key) => key.kid === oldKid));
    const rotated = await authorize(page, origin, ca, first, principal);
    validateIdToken(rotated.token, keys, {
      ...rotated.expected,
      now: Math.floor(Date.now() / 1000),
    });
    assert.equal(
      idTokenKeyId(rotated.token, {
        ...rotated.expected,
        now: Math.floor(Date.now() / 1000),
      }),
      staged.kid,
    );
    await receiver.assertWarm(oldKid);
    assert.deepEqual(receiver.evidence().kids, [oldKid]);
    assert.deepEqual(receiver.sessions(), [
      "newer",
      "old-a",
      "old-b",
      "old-tab",
      "unrelated",
    ]);

    assert.equal(
      (await api(page, "/api/auth/logout", undefined, "POST")).status,
      204,
    );
    assert.equal((await api(page, "/api/auth/session")).status, 401);
    await waitForSessions(receiver, ["newer", "unrelated"], signal);
    const refresh = receiver.refreshEvidence();
    assert.equal(
      refresh.reason,
      "unknown-key",
      "delivery must refresh a still-fresh snapshot",
    );
    assert.equal(refresh.kid, staged.kid);
    assert.ok(refresh.startedAt < refresh.previousExpiresAt);
    const evidence = receiver.evidence();
    assert.equal(
      evidence.fetches,
      2,
      "two actual Logout Tokens share one unknown-kid refresh against /jwks",
    );
    assert.equal(evidence.deliveries.length, 2);
    for (const [client, binding] of [
      [first.client, a],
      [second.client, b],
    ])
      assert.ok(
        evidence.deliveries.some(
          (entry) =>
            entry.client === client &&
            entry.sid === binding.claims.sid &&
            entry.kid === staged.kid,
        ),
      );
    assert.equal((await bind(receiver, "late-old", first, oldA)).bound, false);
    assert.equal(
      (await bind(receiver, "late-rotated", first, rotated)).bound,
      false,
    );
    assert.equal((await api(newerPage, "/api/auth/session")).status, 200);
    assert.equal((await api(otherPage, "/api/auth/session")).status, 200);
    assert.deepEqual(receiver.sessions(), ["newer", "unrelated"]);
    // Leave the main browser authenticated and no extra administrator session for
    // subsequent directory/session scenarios. These are ordinary session logouts.
    assert.equal(
      (await api(newerPage, "/api/auth/logout", undefined, "POST")).status,
      204,
    );
    assert.equal(
      (await api(otherPage, "/api/auth/logout", undefined, "POST")).status,
      204,
    );
    await login(page, origin, "browser@example.com", password);
  } catch (error) {
    failure = error;
    throw error;
  } finally {
    await closeBrowserResources(
      [() => newerContext?.close(), () => otherContext?.close()],
      failure,
    );
  }
  console.log(
    "Real Rust HTTPS discovery/JWKS, two-audience issued tokens, supported publication/activation, unknown-kid refresh and targeted logout/session isolation passed; retiring-key removal remains outside this scenario.",
  );
}
