import { createServer } from "node:https";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import assert from "node:assert/strict";
import { run } from "./lib/command.mjs";
import { createCertificates } from "./lib/deployment-tls.mjs";

const directory = await mkdtemp(join(tmpdir(), "darkhorse-logout-"));
const key = join(directory, "edge.key"),
  cert = join(directory, "edge.pem");
const seen = [];
let server;
const abort = new AbortController();
for (const signal of ["SIGINT", "SIGTERM"])
  process.once(signal, () => abort.abort());
try {
  await createCertificates(directory, "localhost", (name, args, options) =>
    run(name, args, { ...options, signal: abort.signal }),
  );
  server = createServer(
    { key: await readFile(key), cert: await readFile(cert) },
    async (request, response) => {
      let body = "";
      for await (const chunk of request) {
        body += chunk;
        if (body.length > 8192) {
          request.destroy();
          return;
        }
      }
      seen.push({
        url: request.url,
        method: request.method,
        type: request.headers["content-type"],
        body,
      });
      if (request.url === "/slow") return;
      if (request.url === "/headers") {
        response.writeHead(
          200,
          Object.fromEntries(
            Array.from({ length: 40 }, (_, i) => [`x-fixture-${i}`, "test"]),
          ),
        );
        response.end();
        return;
      }
      const status = Number(request.url.slice(1));
      response.writeHead(status, {
        location: "https://127.0.0.1:1/forbidden",
        "cache-control": "no-store",
      });
      if (request.url === "/200") {
        response.write("ignored response body");
        return;
      }
      response.end();
    },
  );
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = server.address().port;
  await run(
    "cargo",
    [
      "test",
      "-p",
      "darkhorse-adapters",
      "--locked",
      "--offline",
      "--features",
      "logout-tests",
      "--test",
      "logout_https",
    ],
    {
      signal: abort.signal,
      env: {
        ...process.env,
        DARKHORSE_TEST_LOGOUT_PORT: String(port),
        DARKHORSE_TEST_LOGOUT_CA: join(directory, "ca.pem"),
        HTTPS_PROXY: "http://127.0.0.1:1",
        ALL_PROXY: "http://127.0.0.1:1",
      },
    },
  );
  assert.deepEqual(
    seen.map((r) => r.url),
    ["/200", "/204", "/503", "/429", "/400", "/302", "/headers", "/slow"],
  );
  for (const request of seen) {
    assert.equal(request.method, "POST");
    assert.equal(request.type, "application/x-www-form-urlencoded");
    assert.equal(
      new URLSearchParams(request.body).get("logout_token"),
      "fixture.token+value/signature",
    );
  }
  console.log(
    "Logout HTTPS boundary passed: trusted TLS, hostname validation, pinned DNS, explicit private approval, no proxy/redirect, bounded response and timeout.",
  );
} finally {
  server?.closeAllConnections();
  if (server) await new Promise((resolve) => server.close(resolve));
  await rm(directory, { recursive: true, force: true });
}
