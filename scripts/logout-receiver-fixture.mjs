// Private stdin/stdout control channel for the PostgreSQL integration test.
// The HTTPS listener exposes logout only; fixture control is never an HTTP route.
import { createServer } from "node:https";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { DatabaseSync } from "node:sqlite";
import { createCertificates } from "./lib/deployment-tls.mjs";
import { run } from "./lib/command.mjs";
import { idTokenKeyId, validateIdToken } from "./lib/reference-client.mjs";
import { receiveLogoutWithJwks } from "./lib/reference-logout-receiver.mjs";
import { createLogoutStore } from "./lib/reference-logout-store.mjs";
import {
  createJwksSource,
  trustedJwksConfiguration,
  parseJwks,
  JWKS_LIMITS,
} from "./lib/reference-jwks.mjs";

const directory = await mkdtemp(join(tmpdir(), "darkhorse-rp-"));
const clock = () => Math.floor(Date.now() / 1000);
const contexts = new Map();
let database,
  store,
  server,
  jwksServer,
  source,
  trusted,
  published = JSON.stringify({ keys: [] }),
  jwksUnavailable = false,
  jwksRequests = 0,
  lostReply = false,
  unavailable = false;
function openStore() {
  database = new DatabaseSync(join(directory, "sessions.sqlite"));
  store = createLogoutStore(database, clock);
}
async function body(request) {
  const chunks = [];
  let bytes = 0;
  for await (const chunk of request) {
    bytes += chunk.length;
    if (bytes > 16384) throw new Error("Body limit.");
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}
async function receive(request, response) {
  try {
    const context = contexts.get(request.url);
    if (!context) {
      response.writeHead(404).end();
      return;
    }
    const input = {
      method: request.method,
      type: request.headers["content-type"],
      body: await body(request),
    };
    const status = unavailable
      ? 503
      : await receiveLogoutWithJwks(input, context, store, source, clock);
    if (status === 204 && lostReply) {
      lostReply = false;
      response.destroy();
      return;
    }
    response.writeHead(status, { "cache-control": "no-store" }).end();
  } catch {
    response.writeHead(400, { "cache-control": "no-store" }).end();
  }
}
async function control(input) {
  switch (input.command) {
    case "configure": {
      const configuration = trustedJwksConfiguration({
        issuer: input.issuer,
        url: input.jwksUrl,
        ca: input.ca,
      });
      if (
        Object.hasOwn(input, "keys") ||
        (trusted &&
          JSON.stringify(configuration) !== JSON.stringify(trusted)) ||
        (!contexts.has(input.path) && contexts.size >= 2)
      )
        throw new Error("Invalid fixture trust configuration.");
      if (!source) {
        trusted = configuration;
        source = createJwksSource(trusted);
      }
      contexts.set(input.path, {
        issuer: input.issuer,
        client: input.client,
      });
      return true;
    }
    case "publish":
      // The private channel controls the separate simulated provider publisher.
      // Receiver caches change only after a verified HTTPS response.
      published = JSON.stringify({
        keys: parseJwks(JSON.stringify({ keys: input.keys })),
      });
      return true;
    case "jwks-unavailable":
      jwksUnavailable = input.value;
      return true;
    case "jwks-requests":
      return jwksRequests;
    case "bind": {
      const context = contexts.get(input.path);
      const expected = {
        ...context,
        subject: input.subject,
        nonce: input.nonce,
        now: clock(),
      };
      const kid = idTokenKeyId(input.token, expected);
      const keys = await source.getKeys(context.issuer, kid);
      const claims = validateIdToken(input.token, keys, {
        ...expected,
        now: clock(),
      });
      return store.bind(input.id, claims);
    }
    case "sessions":
      return store.sessions();
    case "restart":
      database.close();
      openStore();
      return true;
    case "lose-reply":
      lostReply = true;
      return true;
    case "unavailable":
      unavailable = input.value;
      return true;
    default:
      throw new Error("Unknown fixture command.");
  }
}
const input = createInterface({ input: process.stdin });
const abort = new AbortController();
function stop() {
  abort.abort();
  input.close();
  process.stdin.destroy();
}
const deadline = setTimeout(stop, 120000);
for (const signal of ["SIGINT", "SIGTERM"]) process.once(signal, stop);
function reply(value) {
  process.stdout.write(JSON.stringify(value) + "\n");
}
try {
  await createCertificates(directory, "localhost", (name, args, options) =>
    run(name, args, { ...options, signal: abort.signal }),
  );
  openStore();
  const tls = {
    key: await readFile(join(directory, "edge.key")),
    cert: await readFile(join(directory, "edge.pem")),
    maxHeaderSize: 8192,
  };
  server = createServer(tls, receive);
  jwksServer = createServer(tls, (request, response) => {
    if (request.method !== "GET" || request.url !== "/jwks") {
      response.writeHead(404).end();
      return;
    }
    jwksRequests++;
    response.writeHead(jwksUnavailable ? 503 : 200, {
      "content-type": "application/jwk-set+json",
      "cache-control": "no-store",
    });
    response.end(jwksUnavailable ? "" : published);
  });
  for (const listener of [server, jwksServer]) {
    listener.requestTimeout = 5000;
    listener.headersTimeout = 5000;
    listener.timeout = 5000;
    listener.maxConnections = 32;
    await new Promise((resolve) => listener.listen(0, "127.0.0.1", resolve));
  }
  reply({
    origin: `https://localhost:${server.address().port}`,
    jwksUrl: `https://localhost:${jwksServer.address().port}/jwks`,
    refreshCooldownMs: JWKS_LIMITS.cooldownMs,
    ca: await readFile(join(directory, "ca.pem"), "utf8"),
  });
  for await (const line of input) {
    try {
      reply({ result: await control(JSON.parse(line)) });
    } catch {
      reply({ error: "Reference receiver operation failed." });
    }
  }
} finally {
  clearTimeout(deadline);
  for (const listener of [server, jwksServer]) {
    listener?.closeAllConnections();
    if (listener) await new Promise((resolve) => listener.close(resolve));
  }
  database?.close();
  await rm(directory, { recursive: true, force: true });
}
