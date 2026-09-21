import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { execute, successful } from "./lib/security-command.mjs";
import {
  inspectScan,
  verifyTools,
  qualificationStatus,
} from "./lib/security-audit.mjs";

process.chdir(resolve(import.meta.dirname, ".."));
const directory = resolve(".local/security");
const inputs = [
  "Cargo.lock",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
  "package.json",
  "apps/console/package.json",
  ".cargo/audit.toml",
];
const digest = (data) => createHash("sha256").update(data).digest("hex");
const json = (value) => JSON.stringify(value, null, 2) + "\n";

async function save(name, value) {
  await writeFile(resolve(directory, name), json(value), { mode: 0o600 });
}
async function provenance() {
  const hashes = {};
  for (const path of inputs) hashes[path] = digest(await readFile(path));
  return {
    startedAt: new Date().toISOString(),
    revision: (await successful("git", ["rev-parse", "HEAD"])).trim(),
    trackedChanges:
      (
        await successful("git", [
          "status",
          "--porcelain",
          "--untracked-files=no",
        ])
      ).length > 0,
    inputs: hashes,
    tools: verifyTools(
      await successful("cargo", ["audit", "--version"]),
      await successful("pnpm", ["--version"]),
    ),
  };
}
async function scan(name, file, args, parser) {
  const result = await execute(file, args);
  const report = inspectScan(parser, result.stdout, result.code);
  await save(`${name}.json`, report);
  return report;
}
async function main() {
  await mkdir(directory, { recursive: true, mode: 0o700 });
  // Replace any previous report before starting: an interrupted run cannot reuse a pass.
  await save("report.json", {
    version: 1,
    status: "error",
    reason: "Audit did not complete.",
  });
  const evidence = await provenance();
  const rust = await scan(
    "rust",
    "cargo",
    [
      "audit",
      "--json",
      "--deny",
      "warnings",
      "--db",
      resolve(directory, "advisory-db"),
      "--url",
      "https://github.com/RustSec/advisory-db.git",
    ],
    "rust",
  );
  const node = await scan(
    "node",
    "pnpm",
    ["audit", "--json", "--registry=https://registry.npmjs.org"],
    "node",
  );
  const status = qualificationStatus(rust, node);
  await save("report.json", {
    version: 1,
    ...evidence,
    completedAt: new Date().toISOString(),
    status,
    rust,
    node,
  });
  console.log(
    `Dependency checks: Rust ${rust.status}; JavaScript ${node.status}. Evidence: .local/security/report.json`,
  );
  if (status !== "pass") process.exitCode = 1;
}
main().catch(() => {
  console.error(
    "Dependency qualification failed; verify pinned tools, network access and .local/security/report.json. No passing result is available.",
  );
  process.exitCode = 1;
});
