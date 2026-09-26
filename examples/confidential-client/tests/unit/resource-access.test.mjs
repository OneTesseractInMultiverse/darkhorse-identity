import assert from "node:assert/strict";
import { test } from "node:test";
import {
  hasResourceCapability,
  requestResourceIntrospection,
} from "../../src/resource-access.mjs";

const issuer = "https://identity.example.test";
const audience = "urn:darkhorse:resource:12345678-1234-4234-8234-123456789abc";
const introspectionClientId = "rs_12345678-1234-4234-8234-123456789abc";
const introspectionSecret = "ab".repeat(32);
const accessToken = "da_" + "cd".repeat(32);
const capability = "87654321-4321-4321-8321-123456789abc";

function activeResponse(overrides = {}) {
  return {
    status: 200,
    body: {
      active: true,
      token_type: "Bearer",
      iss: issuer,
      aud: audience,
      client_id: "application-client",
      sub: "subject",
      scope: "openid operate",
      iat: 1000,
      exp: 1300,
      capabilities: [capability],
      ...overrides,
    },
  };
}

function expectedAccess(overrides = {}) {
  return {
    issuer,
    audience,
    capability,
    nowSeconds: 1100,
    ...overrides,
  };
}

function rejectBodyCancellation(response) {
  response.body.cancel = async () => {
    throw new Error("cancel failed");
  };
  return response;
}

function rejectReaderCancellation(response) {
  const body = response.body;
  const getReader = body.getReader.bind(body);
  body.getReader = () => {
    const reader = getReader();
    reader.cancel = async () => {
      throw new Error("cancel failed");
    };
    return reader;
  };
  return response;
}

test("resource introspection uses verified HTTPS, Basic resource credentials and a form body", async () => {
  let request;
  const result = await requestResourceIntrospection({
    issuer,
    introspectionClientId,
    introspectionSecret,
    accessToken,
    fetch: async (input, init) => {
      request = { url: new URL(String(input)), init };
      return Response.json({ active: true, iss: issuer, aud: audience });
    },
  });

  assert.equal(request.url.href, `${issuer}/introspect`);
  assert.equal(request.init.method, "POST");
  assert.equal(request.init.redirect, "error");
  assert.equal(request.init.cache, "no-store");
  assert.equal(request.init.credentials, "omit");
  assert.equal(request.init.headers.accept, "application/json");
  assert.equal(
    request.init.headers["content-type"],
    "application/x-www-form-urlencoded",
  );
  assert.equal(
    request.init.headers.authorization,
    `Basic ${Buffer.from(`${introspectionClientId}:${introspectionSecret}`).toString("base64")}`,
  );
  assert.equal(Object.hasOwn(request.init.headers, "cookie"), false);
  assert.equal(Object.hasOwn(request.init.headers, "origin"), false);
  assert.deepEqual(
    [...new URLSearchParams(request.init.body)],
    [
      ["token", accessToken],
      ["token_type_hint", "access_token"],
    ],
  );
  assert.deepEqual(result, {
    status: 200,
    body: { active: true, iss: issuer, aud: audience },
  });
});

test("resource introspection fails closed on network, HTTP and malformed or oversized responses", async () => {
  let calls = 0;
  const request = (fetch) =>
    requestResourceIntrospection({
      issuer,
      introspectionClientId,
      introspectionSecret,
      accessToken,
      fetch,
    });

  assert.deepEqual(
    await request(async () => {
      calls++;
      throw new Error("network");
    }),
    { status: 0, body: null },
  );
  assert.equal(calls, 1, "an uncertain introspection request is never retried");
  assert.deepEqual(
    await request(async () => new Response("denied", { status: 503 })),
    { status: 503, body: null },
  );
  assert.deepEqual(
    await request(async () =>
      rejectBodyCancellation(new Response("denied", { status: 503 })),
    ),
    { status: 503, body: null },
  );
  assert.deepEqual(await request(async () => new Response("not json")), {
    status: 200,
    body: null,
  });
  assert.deepEqual(
    await request(
      async () =>
        new Response("not json", {
          headers: { "content-type": "application/json" },
        }),
    ),
    { status: 200, body: null },
  );
  assert.deepEqual(
    await request(
      async () =>
        new Response(null, {
          headers: { "content-type": "application/json" },
        }),
    ),
    { status: 200, body: null },
  );
  assert.deepEqual(await request(async () => Response.json([])), {
    status: 200,
    body: null,
  });
  assert.deepEqual(await request(async () => Response.json(3)), {
    status: 200,
    body: null,
  });
  assert.deepEqual(
    await request(
      async () =>
        new Response("{}", { headers: { "content-type": "text/plain" } }),
    ),
    { status: 200, body: null },
  );
  assert.deepEqual(
    await request(async () =>
      rejectBodyCancellation(
        new Response("{}", { headers: { "content-type": "text/plain" } }),
      ),
    ),
    { status: 200, body: null },
  );
  assert.deepEqual(
    await request(
      async () =>
        new Response(Uint8Array.from([0xff]), {
          headers: { "content-type": "application/json" },
        }),
    ),
    { status: 200, body: null },
  );
  assert.deepEqual(
    await request(async () => new Response("x".repeat(20_000))),
    { status: 200, body: null },
  );
  assert.deepEqual(
    await request(
      async () =>
        new Response("x".repeat(20_000), {
          headers: { "content-type": "application/json" },
        }),
    ),
    { status: 200, body: null },
  );
  assert.deepEqual(
    await request(async () =>
      rejectReaderCancellation(
        new Response("x".repeat(20_000), {
          headers: { "content-type": "application/json" },
        }),
      ),
    ),
    { status: 200, body: null },
  );
  assert.deepEqual(
    await request(async () =>
      rejectBodyCancellation(
        new Response("{}", {
          headers: {
            "content-type": "application/json",
            "content-length": "20000",
          },
        }),
      ),
    ),
    { status: 200, body: null },
  );
  assert.deepEqual(
    await request(
      async () =>
        new Response("{}", {
          headers: {
            "content-type": "application/json",
            "content-length": "not-a-number",
          },
        }),
    ),
    { status: 200, body: {} },
  );
  assert.deepEqual(
    await request(
      async () =>
        new Response("{}", {
          headers: {
            "content-type": "Application/JSON; charset=utf-8",
            "content-length": "2",
          },
        }),
    ),
    { status: 200, body: {} },
  );
  const missingContentType = Response.json({});
  missingContentType.headers.delete("content-type");
  assert.deepEqual(await request(async () => missingContentType), {
    status: 200,
    body: null,
  });
  assert.deepEqual(
    await request(async () => new Response(null, { status: 503 })),
    { status: 503, body: null },
  );
  assert.deepEqual(
    await request(
      async () =>
        new Response(
          new ReadableStream({
            pull(controller) {
              controller.error(new Error("body read failed"));
            },
          }),
          { headers: { "content-type": "application/json" } },
        ),
    ),
    { status: 0, body: null },
  );
  assert.deepEqual(
    await requestResourceIntrospection({
      issuer,
      introspectionClientId,
      introspectionSecret,
      accessToken,
      timeoutMs: 100,
      fetch: (_input, { signal }) =>
        new Promise((_resolve, reject) => {
          signal.addEventListener(
            "abort",
            () => reject(new Error("request timeout")),
            { once: true },
          );
        }),
    }),
    { status: 0, body: null },
  );
});

test("resource introspection rejects invalid configuration without making a request", async () => {
  let calls = 0;
  const fetch = async () => {
    calls++;
    return Response.json({ active: false });
  };
  const base = {
    issuer,
    introspectionClientId,
    introspectionSecret,
    accessToken,
    fetch,
  };
  for (const overrides of [
    { issuer: null },
    { issuer: "http://identity.example.test" },
    { issuer: "https://user:pass@identity.example.test" },
    { issuer: "https://identity.example.test?token=secret" },
    { issuer: "https://identity.example.test#fragment" },
    { issuer: "https://identity.example.test/tenant" },
    { issuer: "https://identity.example.test/" },
    { issuer: "https://identity.example.test:443" },
    { issuer: "://invalid" },
    { introspectionClientId: "client" },
    { introspectionClientId: null },
    { introspectionClientId: "rs_12345678-1234-4234-8234-123456789abC" },
    { introspectionSecret: "secret" },
    { introspectionSecret: "AB".repeat(32) },
    { accessToken: "" },
    { accessToken: null },
    { accessToken: "token with spaces" },
    { accessToken: "x".repeat(2049) },
    { fetch: null },
    { timeoutMs: 99 },
    { timeoutMs: 10_001 },
    { timeoutMs: 100.5 },
  ]) {
    await assert.rejects(
      requestResourceIntrospection({ ...base, ...overrides }),
      /resource introspection configuration is invalid/,
    );
  }
  assert.equal(calls, 0);
});

test("capability policy requires an active exact-issuer, exact-audience, live resource grant", () => {
  assert.equal(hasResourceCapability(activeResponse(), expectedAccess()), true);
  const denied = [
    { status: 503, body: null },
    { status: 200, body: { active: false } },
    activeResponse({ iss: "https://other.example.test" }),
    activeResponse({ aud: "urn:darkhorse:resource:other" }),
    activeResponse({ exp: 1100 }),
    activeResponse({ iat: 1101 }),
    activeResponse({ iat: 1000, exp: 1401 }),
    activeResponse({ capabilities: [] }),
    activeResponse({ capabilities: ["other-capability"] }),
    activeResponse({ token_type: "Logout" }),
  ];
  for (const result of denied)
    assert.equal(hasResourceCapability(result, expectedAccess()), false);
  assert.equal(hasResourceCapability(null, expectedAccess()), false);
  assert.equal(hasResourceCapability(activeResponse(), null), false);
  assert.equal(hasResourceCapability(activeResponse(), []), false);
  assert.equal(
    hasResourceCapability(
      activeResponse(),
      expectedAccess({ nowSeconds: NaN }),
    ),
    false,
  );
  assert.equal(
    hasResourceCapability(activeResponse(), expectedAccess({ capability: "" })),
    false,
  );
  assert.equal(
    hasResourceCapability(activeResponse({ iat: -1 }), expectedAccess()),
    false,
  );
  assert.equal(
    hasResourceCapability(activeResponse({ exp: 1300.5 }), expectedAccess()),
    false,
  );
  assert.equal(
    hasResourceCapability(
      activeResponse({ capabilities: ["invalid"] }),
      expectedAccess(),
    ),
    false,
  );
  assert.equal(
    hasResourceCapability(
      activeResponse({ capabilities: Array(257).fill(capability) }),
      expectedAccess(),
    ),
    false,
  );
  assert.equal(
    hasResourceCapability(
      activeResponse(),
      expectedAccess({ audience: "bad" }),
    ),
    false,
  );
  assert.equal(
    hasResourceCapability(
      activeResponse(),
      expectedAccess({ issuer: "http://identity.example.test" }),
    ),
    false,
  );
  assert.equal(
    hasResourceCapability(
      activeResponse(),
      expectedAccess({ issuer: "://invalid" }),
    ),
    false,
  );
  assert.equal(
    hasResourceCapability({ status: 200, body: [] }, expectedAccess()),
    false,
  );
});
