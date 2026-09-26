import { randomBytes, createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { platform, arch, availableParallelism, totalmem } from "node:os";
import {
  boundarySuite,
  boundaryResult,
  toolVersions,
} from "./lib/boundary-ci.mjs";
import { boundaryProcess } from "./lib/boundary-process.mjs";
import { cleanupBoundary } from "./lib/boundary-cleanup.mjs";
import {
  postgresImage,
  redisImage,
  objectsImage,
} from "./lib/boundary-images.mjs";
import { successful } from "./lib/security-command.mjs";

process.chdir(resolve(import.meta.dirname, ".."));
const cleanupOnly = process.argv[2] === "cleanup";
const suite = boundarySuite(process.argv.slice(cleanupOnly ? 3 : 2));
const directory = resolve(".local/ci-boundary", suite);
const abort = new AbortController();
for (const signal of ["SIGINT", "SIGTERM"])
  process.once(signal, () => abort.abort());
const save = (name, value) =>
  writeFile(resolve(directory, name), JSON.stringify(value, null, 2) + "\n", {
    mode: 0o600,
  });

async function provenance() {
  const inputs = {};
  for (const file of [
    "Cargo.lock",
    "rust-toolchain.toml",
    "Makefile",
    ".github/workflows/ci.yaml",
    "scripts/postgres-test.mjs",
    "scripts/redis-test.mjs",
    "scripts/browser-test.mjs",
    "scripts/lib/browser-evidence.mjs",
    "scripts/lib/objects-test-service.mjs",
    "pnpm-lock.yaml",
    "package.json",
  ])
    inputs[file] = createHash("sha256")
      .update(await readFile(file))
      .digest("hex");
  return {
    revision: (await successful("git", ["rev-parse", "HEAD"])).trim(),
    trackedChanges:
      (
        await successful("git", [
          "status",
          "--porcelain",
          "--untracked-files=no",
        ])
      ).length > 0,
    inputs,
    runner: {
      os: platform(),
      architecture: arch(),
      parallelism: availableParallelism(),
      memoryMiB: Math.floor(totalmem() / 1024 / 1024),
    },
    images:
      suite === "postgres"
        ? [postgresImage]
        : [postgresImage, redisImage, objectsImage],
    tools: {
      node: process.version,
      ...toolVersions({
        rust: (await successful("rustc", ["--version"])).trim(),
        docker: (
          await successful("docker", [
            "version",
            "--format",
            "{{.Client.Version}} / {{.Server.Version}}",
          ])
        ).trim(),
        openssl: (await successful("openssl", ["version"])).trim(),
      }),
    },
  };
}
async function cleanup() {
  const owner = JSON.parse(
    await readFile(resolve(directory, "owner.json"), "utf8"),
  );
  let status;
  try {
    status = await cleanupBoundary(owner);
  } catch {
    status = { status: "failed" };
  }
  const report = JSON.parse(
    await readFile(resolve(directory, "report.json"), "utf8"),
  );
  await save("report.json", {
    ...report,
    cleanup: status,
    ...(status.status === "failed" ? { status: "failed" } : {}),
  });
  if (status.status !== "verified")
    throw new Error("Owned resource cleanup failed.");
}
async function run() {
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const owner = randomBytes(16).toString("hex");
  const startedAt = new Date().toISOString();
  await save("report.json", {
    version: 1,
    suite,
    status: "incomplete",
    startedAt,
    cleanup: { status: "pending" },
  });
  await save("owner.json", owner);
  const evidence = await provenance();
  await save("report.json", {
    version: 1,
    suite,
    status: "incomplete",
    startedAt,
    ...evidence,
    cleanup: { status: "pending" },
  });
  console.log(
    `Running full ${suite} boundary suite; private child output is bounded and is not published.`,
  );
  let report;
  try {
    const result = await boundaryProcess("make", [`test-${suite}`], {
      env: {
        ...process.env,
        DARKHORSE_TEST_RUN_ID: owner,
        DARKHORSE_TEST_BROWSER: suite === "browser" ? "true" : "false",
        CARGO_TERM_COLOR: "never",
      },
      signal: abort.signal,
      timeoutMs: (suite === "browser" ? 35 : 25) * 60_000,
    });
    report = {
      version: 1,
      suite,
      startedAt,
      finishedAt: new Date().toISOString(),
      ...evidence,
      ...boundaryResult(suite, result),
      cleanup: { status: "pending" },
    };
    await save("report.json", report);
  } finally {
    await cleanup();
  }
  console.log(
    JSON.stringify({
      suite,
      status: report.status,
      suites: report.suites,
      worker: report.worker,
      browser: report.browser,
      failedTests: report.failedTests,
    }),
  );
  if (report.status !== "passed")
    throw new Error(
      "Boundary suite failed or returned incomplete evidence. Inspect report.json; reproduce locally for private diagnostics.",
    );
}
(cleanupOnly ? cleanup() : run()).catch(() => {
  console.error(
    "Boundary verification or owned cleanup failed. No private child output was published.",
  );
  process.exitCode = 1;
});
