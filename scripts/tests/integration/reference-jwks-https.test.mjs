import { test } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:https";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createCertificates } from "../../lib/deployment-tls.mjs";
import { run } from "../../lib/command.mjs";
import { createJwksTransport } from "../../lib/reference-jwks-https.mjs";
import { JWKS_LIMITS } from "../../lib/reference-jwks.mjs";

test(
  "local HTTPS JWKS enforces CA/hostname trust, redirects, bytes and an absolute body deadline",
  { timeout: 30000 },
  async () => {
    const directory = await mkdtemp(join(tmpdir(), "darkhorse-jwks-"));
    let server,
      timer,
      requests = 0;
    try {
      await createCertificates(directory, "localhost", run);
      const ca = await readFile(join(directory, "ca.pem"), "utf8");
      server = createServer(
        {
          key: await readFile(join(directory, "edge.key")),
          cert: await readFile(join(directory, "edge.pem")),
        },
        (req, res) => {
          requests++;
          res.setHeader("content-type", "application/jwk-set+json");
          if (req.url === "/redirect") {
            res.writeHead(302, { location: "/good" }).end();
            return;
          }
          if (req.url === "/large-header") {
            res.setHeader("x-large", "x".repeat(9000));
            res.end("{}");
            return;
          }
          if (req.url === "/length") {
            res.setHeader("content-length", 65537);
            res.end();
            return;
          }
          if (req.url === "/chunked") {
            res.write("x".repeat(40000));
            res.end("x".repeat(40000));
            return;
          }
          if (req.url === "/truncated") {
            res.setHeader("content-length", 100);
            res.write("{");
            setImmediate(() => res.destroy());
            return;
          }
          if (req.url === "/drip") {
            res.write("{");
            timer = setInterval(() => res.write(" "), 100);
            res.on("close", () => clearInterval(timer));
            return;
          }
          if (req.url === "/stall") return;
          res.end('{"keys":[]}');
        },
      );
      await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
      const origin = `https://localhost:${server.address().port}`;
      const fetch = (path, authority = ca, base = origin) =>
        createJwksTransport()({ url: base + path, ca: authority });
      assert.equal((await fetch("/good")).toString(), '{"keys":[]}');
      await assert.rejects(fetch("/good", ""));
      await assert.rejects(createJwksTransport()({ url: origin + "/good" }));
      await assert.rejects(
        fetch("/good", ca, origin.replace("localhost", "127.0.0.1")),
      );
      const before = requests;
      await assert.rejects(fetch("/redirect"));
      assert.equal(requests, before + 1, "redirect was followed");
      for (const path of ["/large-header", "/length", "/chunked", "/truncated"])
        await assert.rejects(fetch(path));
      for (const path of ["/drip", "/stall"]) {
        const started = performance.now();
        await assert.rejects(fetch(path));
        assert.ok(performance.now() - started < JWKS_LIMITS.timeoutMs + 1000);
      }
    } finally {
      clearInterval(timer);
      server?.closeAllConnections();
      if (server) await new Promise((resolve) => server.close(resolve));
      await rm(directory, { recursive: true, force: true });
    }
  },
);
