import { test } from "node:test";
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { createJwksTransport } from "../../lib/reference-jwks-https.mjs";
import {
  JWKS_LIMITS,
  validJwksResponse,
} from "../../lib/reference-jwks-policy.mjs";

test("HTTPS response policy rejects redirects, encoding, unbounded lengths and headers", () => {
  const headers = { "content-type": "application/jwk-set+json" };
  assert.equal(validJwksResponse(200, headers, 1), true);
  assert.equal(
    validJwksResponse(
      200,
      {
        "content-type": "application/json; charset=utf-8",
        "content-encoding": "identity",
        "content-length": "65536",
      },
      32,
    ),
    true,
  );
  for (const [status, changes, count] of [
    [302, {}, 1],
    [503, {}, 1],
    [304, {}, 1],
    [200, { "content-type": "text/html" }, 1],
    [200, { "content-type": undefined }, 1],
    [200, { "content-encoding": "gzip" }, 1],
    [200, { "content-length": "65537" }, 1],
    [200, { "content-length": "-1" }, 1],
    [200, { "content-length": "NaN" }, 1],
    [200, {}, 33],
  ])
    assert.equal(
      validJwksResponse(status, { ...headers, ...changes }, count),
      false,
    );
});

test("the absolute fetch deadline releases callers but retains the sole permit until OS DNS finishes", async () => {
  let deadline,
    resolved,
    requests = 0;
  const transport = createJwksTransport({
    schedule: (callback, ms) => {
      assert.equal(ms, JWKS_LIMITS.timeoutMs);
      deadline = callback;
      return 1;
    },
    cancel: (id) => assert.equal(id, 1),
    resolveHost: (_host, _options, callback) => {
      resolved = callback;
    },
    send: (_url, options) => {
      requests++;
      assert.equal(options.rejectUnauthorized, true);
      assert.equal(options.agent, false);
      const req = new EventEmitter();
      req.end = () => options.lookup("issuer.example", {}, () => {});
      req.destroy = () => req.emit("close");
      return req;
    },
  });
  const first = transport({ url: "https://issuer.example/jwks" });
  deadline();
  await assert.rejects(first);
  for (let i = 0; i < 10; i++) await assert.rejects(transport({}));
  assert.equal(requests, 1);
  resolved(null, "127.0.0.1", 4);
  const second = transport({ url: "https://issuer.example/jwks" });
  assert.equal(requests, 2);
  resolved(new Error("DNS failure"));
  deadline();
  await assert.rejects(second);
});

test("synchronous request setup failures clear their deadline and release admission", async () => {
  let cancelled = 0,
    attempts = 0;
  const transport = createJwksTransport({
    schedule: () => 1,
    cancel: () => {
      cancelled++;
    },
    send: () => {
      attempts++;
      throw new Error("setup failed");
    },
  });
  await assert.rejects(transport({ url: "https://issuer.example/jwks" }));
  await assert.rejects(transport({ url: "https://issuer.example/jwks" }));
  assert.equal(attempts, 2);
  assert.equal(cancelled, 2);
});
