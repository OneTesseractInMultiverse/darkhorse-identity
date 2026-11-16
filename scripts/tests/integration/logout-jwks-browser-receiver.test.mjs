import { test } from "node:test";
import assert from "node:assert/strict";
import { createServer, request } from "node:https";
import { generateKeyPairSync, sign } from "node:crypto";
import { mkdtemp, readFile, copyFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createCertificates } from "../../lib/deployment-tls.mjs";
import { run } from "../../lib/command.mjs";
import { discoverJwks } from "../../lib/reference-discovery.mjs";
import { startDeployedLogoutReceiver } from "../../lib/logout-jwks-browser-receiver.mjs";
import { closeBrowserResources } from "../../lib/browser-cleanup.mjs";

test(
  "the browser receiver shares fixture TLS, exact audiences, atomic removal and bounded evidence",
  { timeout: 30000 },
  async () => {
    const directory = await mkdtemp(join(tmpdir(), "darkhorse-browser-jwks-"));
    let provider, receiver, failure;
    try {
      await createCertificates(directory, "localhost", run);
      await copyFile(
        join(directory, "edge.key"),
        join(directory, "server.key"),
      );
      await copyFile(
        join(directory, "edge.pem"),
        join(directory, "server.pem"),
      );
      const ca = await readFile(join(directory, "ca.pem"), "utf8");
      const { publicKey, privateKey } = generateKeyPairSync("rsa", {
        modulusLength: 2048,
      });
      const key = {
        ...publicKey.export({ format: "jwk" }),
        kid: "old",
        alg: "RS256",
        use: "sig",
      };
      let issuer,
        requests = 0;
      provider = createServer(
        {
          key: await readFile(join(directory, "server.key")),
          cert: await readFile(join(directory, "server.pem")),
        },
        (req, res) => {
          res.setHeader("content-type", "application/json");
          if (req.url === "/.well-known/openid-configuration")
            res.end(JSON.stringify({ issuer, jwks_uri: issuer + "/jwks" }));
          else if (req.url === "/jwks") {
            requests++;
            res.end(JSON.stringify({ keys: [key] }));
          } else res.writeHead(404).end();
        },
      );
      await new Promise((resolve) => provider.listen(0, "127.0.0.1", resolve));
      issuer = `https://localhost:${provider.address().port}`;
      receiver = await startDeployedLogoutReceiver(directory, issuer, ca);
      const trusted = await discoverJwks({ issuer, url: issuer + "/jwks", ca });
      assert.throws(() => receiver.configure(trusted, ["a", "a"]));
      receiver.configure(trusted, ["a", "b"]);
      assert.throws(() => receiver.configure(trusted, ["a", "b"]));
      assert.deepEqual(
        JSON.parse(receiver.settings.DARKHORSE_LOGOUT_PRIVATE_DESTINATIONS),
        { [receiver.origin]: ["127.0.0.1", "::1"] },
      );
      assert.equal(receiver.settings.DARKHORSE_LOGOUT_CA_PEM, ca);
      const sid = "12345678-1234-4567-89ab-123456789abc";
      const token = (aud, logout, changes = {}) => {
        const now = Math.floor(Date.now() / 1000);
        const encode = (value) =>
          Buffer.from(JSON.stringify(value)).toString("base64url");
        const payload = {
          iss: issuer,
          aud,
          sub: "subject",
          sid,
          iat: now,
          exp: now + 120,
          ...(logout
            ? {
                jti: "logout-" + aud,
                events: {
                  "http://schemas.openid.net/event/backchannel-logout": {},
                },
              }
            : { auth_time: now, nonce: "nonce" }),
          ...changes,
        };
        const message = `${encode({ alg: "RS256", typ: logout ? "logout+jwt" : "JWT", kid: "old" })}.${encode(payload)}`;
        return `${message}.${sign("RSA-SHA256", Buffer.from(message), privateKey).toString("base64url")}`;
      };
      const id = token("a", false);
      assert.equal(
        (await receiver.bind("a", "a", id, "subject", "nonce")).bound,
        true,
      );
      assert.equal(
        (await receiver.bind("a-tab", "a", id, "subject", "nonce")).bound,
        true,
      );
      assert.equal(
        (await receiver.bind("b", "b", token("b", false), "subject", "nonce"))
          .bound,
        true,
      );
      await assert.rejects(receiver.bind("wrong", "b", id, "subject", "nonce"));
      await receiver.assertWarm("old");
      assert.equal(requests, 1);
      const post = (path, body) =>
        new Promise((resolve, reject) => {
          const req = request(
            receiver.origin + path,
            {
              ca,
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
          req.end(new URLSearchParams({ logout_token: body }).toString());
        });
      assert.equal(await post("/missing", token("a", true)), 404);
      assert.equal(await post("/a", "malformed"), 400);
      assert.equal(await post("/b", token("a", true)), 400);
      assert.deepEqual(receiver.sessions(), ["a", "a-tab", "b"]);
      const logout = token("a", true);
      assert.equal(await post("/a", logout), 204);
      assert.equal(await post("/a", logout), 204);
      assert.deepEqual(receiver.sessions(), ["b"]);
      assert.equal(
        (await receiver.bind("late", "a", id, "subject", "nonce")).bound,
        false,
      );
      for (let index = 0; index < 33; index++)
        assert.equal(await post("/a", logout), 204);
      assert.equal(receiver.evidence().deliveries.length, 32);
      assert.equal(receiver.evidence().fetches, 1);
      const evidence = receiver.evidence();
      evidence.kids.length = 0;
      evidence.deliveries[0].client = "changed";
      assert.deepEqual(receiver.evidence().kids, ["old"]);
      assert.equal(receiver.evidence().deliveries[0].client, "a");
    } catch (error) {
      failure = error;
      throw error;
    } finally {
      await closeBrowserResources(
        [
          () => receiver?.close(),
          async () => {
            provider?.closeAllConnections();
            if (provider)
              await new Promise((resolve) => provider.close(resolve));
          },
          () => rm(directory, { recursive: true, force: true }),
        ],
        failure,
      );
    }
  },
);
