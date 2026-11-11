import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { once } from "node:events";
import { request } from "node:https";
import { generateKeyPairSync, sign } from "node:crypto";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";

function signingKey(kid) {
  const { publicKey, privateKey } = generateKeyPairSync("rsa", {
    modulusLength: 2048,
  });
  return {
    privateKey,
    public: {
      ...publicKey.export({ format: "jwk" }),
      kid,
      kty: "RSA",
      alg: "RS256",
      use: "sig",
    },
  };
}
const issuer = "https://provider.example";
const sid = "12345678-1234-4567-89ab-123456789abc";
function token(key, client, logout, changes = {}, headers = {}) {
  const now = Math.floor(Date.now() / 1000);
  const payload = {
    iss: issuer,
    aud: client,
    sub: "subject",
    sid,
    iat: now,
    exp: now + (logout ? 120 : 300),
    ...(logout
      ? {
          jti: "logout-" + client,
          events: { "http://schemas.openid.net/event/backchannel-logout": {} },
        }
      : { auth_time: now, nonce: "nonce" }),
    ...changes,
  };
  const encode = (value) =>
    Buffer.from(JSON.stringify(value)).toString("base64url");
  const message = `${encode({ alg: "RS256", typ: logout ? "logout+jwt" : "JWT", kid: key.public.kid, ...headers })}.${encode(payload)}`;
  return `${message}.${sign("RSA-SHA256", Buffer.from(message), key.privateKey).toString("base64url")}`;
}
function post(ready, client, token) {
  return new Promise((resolve, reject) => {
    const req = request(
      `${ready.origin}/${client}`,
      {
        ca: ready.ca,
        agent: false,
        method: "POST",
        headers: { "content-type": "application/x-www-form-urlencoded" },
      },
      (res) => {
        res.resume();
        res.on("end", () => resolve(res.statusCode));
      },
    );
    req.on("error", reject);
    req.end(new URLSearchParams({ logout_token: token }).toString());
  });
}

test(
  "two applications refresh over HTTPS through rotation/outage and preserve replay and session isolation",
  { timeout: 45000 },
  async () => {
    const child = spawn(
      process.execPath,
      [
        fileURLToPath(
          new URL("../../logout-receiver-fixture.mjs", import.meta.url),
        ),
      ],
      { stdio: ["pipe", "pipe", "inherit"] },
    );
    const exited = once(child, "exit");
    const lines = createInterface({ input: child.stdout })[
      Symbol.asyncIterator
    ]();
    const read = async () => {
      const { value, done } = await lines.next();
      assert.equal(done, false, "receiver exited before replying");
      return JSON.parse(value);
    };
    const command = async (input) => {
      child.stdin.write(JSON.stringify(input) + "\n");
      const reply = await read();
      assert.equal(reply.error, undefined);
      return reply.result;
    };
    try {
      const ready = await read();
      const old = signingKey("old"),
        next = signingKey("next");
      await command({ command: "publish", keys: [old.public] });
      for (const client of ["a", "b"])
        await command({
          command: "configure",
          path: "/" + client,
          issuer,
          client,
          jwksUrl: ready.jwksUrl,
          ca: ready.ca,
        });
      const first = token(old, "a", false),
        second = token(old, "b", false);
      const bind = (id, client, signed, subject = "subject") =>
        command({
          command: "bind",
          path: "/" + client,
          token: signed,
          id,
          subject,
          nonce: "nonce",
        });
      assert.equal(await bind("old-a", "a", first), true);
      assert.equal(await bind("old-tab", "a", first), true);
      assert.equal(await bind("old-b", "b", second), true);
      assert.equal(
        await bind(
          "newer",
          "a",
          token(old, "a", false, {
            sid: "87654321-4321-4567-89ab-123456789abc",
          }),
        ),
        true,
      );
      assert.equal(
        await bind(
          "unrelated",
          "a",
          token(old, "a", false, { sub: "other" }),
          "other",
        ),
        true,
      );
      assert.equal(
        await command({ command: "jwks-requests" }),
        1,
        "both applications must share one issuer cache",
      );
      await command({ command: "publish", keys: [old.public, next.public] });
      await command({ command: "jwks-unavailable", value: true });
      await delay(ready.refreshCooldownMs + 100);
      const logoutA = token(next, "a", true),
        logoutB = token(next, "b", true);
      assert.equal(await post(ready, "a", logoutA), 503);
      assert.deepEqual(await command({ command: "sessions" }), [
        "newer",
        "old-a",
        "old-b",
        "old-tab",
        "unrelated",
      ]);
      assert.equal(await command({ command: "jwks-requests" }), 2);
      const probe = token(old, "a", true, { sid: "probe", jti: "probe" });
      assert.equal(
        await post(ready, "a", probe),
        204,
        "fresh overlapping key remains usable during outage",
      );
      assert.equal(
        await post(ready, "a", logoutA),
        503,
        "cooldown must prevent another fetch",
      );
      await command({ command: "jwks-unavailable", value: false });
      await delay(ready.refreshCooldownMs + 100);
      assert.deepEqual(
        await Promise.all([
          post(ready, "a", logoutA),
          post(ready, "b", logoutB),
        ]),
        [204, 204],
      );
      assert.equal(
        await command({ command: "jwks-requests" }),
        3,
        "rotation must use one shared refresh",
      );
      assert.deepEqual(await command({ command: "sessions" }), [
        "newer",
        "unrelated",
      ]);
      await command({ command: "restart" });
      assert.equal(await post(ready, "a", logoutA), 204);
      assert.equal(await post(ready, "b", logoutB), 204);
      assert.equal(await bind("late-a", "a", first), false);
      assert.equal(await bind("late-b", "b", second), false);
      assert.equal(
        await post(ready, "a", token(next, "a", true, { sid: "different" })),
        400,
        "conflicting jti must fail",
      );
      assert.equal(await post(ready, "b", logoutA), 400);
      for (const header of [
        { jku: "https://attacker.example" },
        { x5u: "https://attacker.example" },
        { kid: "next" },
      ])
        assert.equal(
          await post(ready, "a", token(old, "a", true, {}, header)),
          400,
        );
      assert.equal(
        await command({ command: "jwks-requests" }),
        3,
        "invalid known-key signatures must not force refresh",
      );
      assert.deepEqual(await command({ command: "sessions" }), [
        "newer",
        "unrelated",
      ]);
    } finally {
      child.stdin.end();
      const [code] = await exited;
      assert.equal(code, 0);
    }
  },
);
