import { access, mkdir } from "node:fs/promises";
import { createServer } from "node:net";
import { request } from "node:https";
import { readFile } from "node:fs/promises";
import { setTimeout as delay } from "node:timers/promises";
import { startProcess, interruption } from "./lib/process.mjs";
import { supervise } from "./lib/supervisor.mjs";

const mode = process.argv[2] ?? "all";
const caddy = process.env.CADDY ?? "caddy";
const certificate = ".local/pki/pki/authorities/local/root.crt";
const specs = {
  api: { command: process.execPath, args: ["scripts/watch-api.mjs"] },
  web: { command: "pnpm", args: ["--filter", "@darkhorse/console", "dev"] },
  proxy: { command: caddy, args: ["run", "--config", "config/Caddyfile"] },
};

async function requireFreePort(port) {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once("error", () =>
      reject(
        new Error(
          `Port ${port} is occupied. Stop its owner before starting development.`,
        ),
      ),
    );
    server.listen(port, "127.0.0.1", resolve);
  });
  await new Promise((resolve) => server.close(resolve));
}

async function prepare() {
  process.umask(0o077);
  await mkdir(".local", { recursive: true, mode: 0o700 });
  await mkdir(".local/pki", { recursive: true, mode: 0o700 });
}

async function setup() {
  await prepare();
  const validation = startProcess({
    command: caddy,
    args: ["validate", "--config", "config/Caddyfile"],
  });
  const result = await validation.done;
  if (result.code !== 0) throw new Error("Caddy validation failed.");
  await access(certificate);
  console.log(
    `Local CA prepared at ${certificate}. No host trust was changed.`,
  );
}

async function httpsCheck(path = "/health/live") {
  const ca = await readFile(certificate);
  await new Promise((resolve, reject) => {
    const req = request(
      `https://localhost:8443${path}`,
      { ca, timeout: 3000 },
      (response) => {
        response.resume();
        response.once("end", () =>
          response.statusCode === 200
            ? resolve()
            : reject(new Error("Rust health endpoint is not ready.")),
        );
      },
    );
    req.on("timeout", () =>
      req.destroy(new Error("HTTPS readiness timed out.")),
    );
    req.on("error", reject);
    req.end();
  });
}

async function readiness(signal) {
  for (let attempt = 0; attempt < 120 && !signal.aborted; attempt++) {
    try {
      await httpsCheck();
      await httpsCheck("/");
      console.log(
        "\nPortal ready: https://localhost:8443 — Ctrl-C stops all owned processes.\n",
      );
      return;
    } catch {
      await delay(500);
    }
  }
  if (!signal.aborted)
    throw new Error(
      "Development startup timed out. Inspect the process output.",
    );
}

async function run() {
  if (mode === "setup") return setup();
  if (mode === "check") {
    await httpsCheck();
    console.log("HTTPS certificate, hostname, and Rust routing verified.");
    return;
  }
  if (!["all", "api", "proxy"].includes(mode))
    throw new Error("Unknown development mode.");
  await prepare();
  if (mode !== "api")
    await access(certificate).catch(() => {
      throw new Error("Run make https-setup before starting the proxy.");
    });
  const ports =
    mode === "all" ? [3001, 5173, 8443] : mode === "api" ? [3001] : [8443];
  for (const port of ports) await requireFreePort(port);
  const controller = interruption();
  const selected = mode === "all" ? Object.values(specs) : [specs[mode]];
  const running = supervise(selected, startProcess, controller.signal);
  if (mode === "all") {
    try {
      await Promise.race([running, readiness(controller.signal)]);
      await running;
    } finally {
      controller.abort();
      await running;
    }
  } else await running;
}

try {
  await run();
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
