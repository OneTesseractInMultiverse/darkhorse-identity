// In-process variant of the reference fixture, sharing its listener, validator,
// bounded JWKS source and atomic SQLite store. The managed browser owner closes it.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import {
  createLogoutServer,
  readLogoutBody,
} from "./reference-logout-http.mjs";
import { createLogoutStore } from "./reference-logout-store.mjs";
import {
  idTokenKeyId,
  logoutTokenKeyId,
  validateIdToken,
} from "./reference-client.mjs";
import {
  parseLogoutRequest,
  receiveLogoutWithJwks,
} from "./reference-logout-receiver.mjs";
import { createJwksSource, JWKS_LIMITS, parseJwks } from "./reference-jwks.mjs";
import { createJwksTransport } from "./reference-jwks-https.mjs";
import { trustedDiscovery } from "./reference-discovery.mjs";

export async function startDeployedLogoutReceiver(directory, issuer, ca) {
  const tls = {
    key: await readFile(join(directory, "server.key")),
    cert: await readFile(join(directory, "server.pem")),
  };
  const database = new DatabaseSync(":memory:");
  const clock = () => Math.floor(Date.now() / 1000);
  const store = createLogoutStore(database, clock);
  const contexts = new Map(),
    deliveries = [];
  let source,
    fetches = 0,
    lastKids = [];
  const receive = async (request, response) => {
    let status = 400;
    try {
      const context = contexts.get(request.url);
      if (!context) status = 404;
      else {
        const input = {
          method: request.method,
          type: request.headers["content-type"],
          body: await readLogoutBody(request),
        };
        const kid = logoutTokenKeyId(parseLogoutRequest(input), {
          ...context,
          now: clock(),
        });
        status = await receiveLogoutWithJwks(
          input,
          context,
          {
            accept(claims, fingerprint) {
              const result = store.accept(claims, fingerprint);
              if (result === "accepted" || result === "duplicate") {
                if (deliveries.length === 32) deliveries.shift();
                deliveries.push({ client: claims.aud, sid: claims.sid, kid });
              }
              return result;
            },
          },
          source,
          clock,
        );
      }
    } catch {}
    response.writeHead(status, { "cache-control": "no-store" }).end();
  };
  let server;
  try {
    server = createLogoutServer(tls, receive);
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(0, "127.0.0.1", resolve);
    });
  } catch (error) {
    server?.closeAllConnections();
    database.close();
    throw error;
  }
  const origin = `https://localhost:${server.address().port}`;
  return {
    origin,
    settings: {
      DARKHORSE_LOGOUT_PRIVATE_DESTINATIONS: JSON.stringify({
        [origin]: ["127.0.0.1", "::1"],
      }),
      DARKHORSE_LOGOUT_CA_PEM: ca,
    },
    configure(trusted, clients) {
      assert.equal(source, undefined, "one immutable deployment authority");
      assert.equal(clients.length, 2);
      assert.equal(new Set(clients).size, 2);
      const configuration = trustedDiscovery(
        { issuer: trusted.issuer, jwks_uri: trusted.url },
        { issuer, url: `${issuer}/jwks`, ca },
      );
      const transport = createJwksTransport();
      source = createJwksSource(configuration, {
        cacheMs: JWKS_LIMITS.maxCacheMs,
        transport: async (input) => {
          fetches++;
          const body = await transport(input);
          lastKids = parseJwks(body).map((key) => key.kid);
          return body;
        },
      });
      for (const client of clients)
        contexts.set(`/${client}`, { issuer, client });
    },
    async bind(id, client, token, subject, nonce) {
      const context = contexts.get(`/${client}`);
      assert.ok(context);
      const expected = { ...context, subject, nonce, now: clock() };
      const kid = idTokenKeyId(token, expected);
      const keys = await source.getKeys(issuer, kid);
      const claims = validateIdToken(token, keys, {
        ...expected,
        now: clock(),
      });
      return { bound: store.bind(id, claims), claims };
    },
    async assertWarm(kid) {
      const before = fetches;
      await source.getKeys(issuer, kid);
      assert.equal(
        fetches,
        before,
        "rotation must encounter a fresh cache, not an expired snapshot",
      );
    },
    sessions: () => store.sessions(),
    evidence: () => ({
      fetches,
      kids: [...lastKids],
      deliveries: deliveries.map((value) => ({ ...value })),
    }),
    refreshEvidence: () => source.lastRefresh(),
    async close() {
      server.closeAllConnections();
      await new Promise((resolve) => server.close(resolve));
      database.close();
    },
  };
}
