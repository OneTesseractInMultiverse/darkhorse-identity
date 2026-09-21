import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execute, successful } from "./lib/security-command.mjs";
import { sourceTree, archivePaths } from "./lib/source-package.mjs";

// Integration fixture: real subprocesses and a disposable Git index; no commit identity required.
const required = [
  "Cargo.toml",
  "Cargo.lock",
  "Makefile",
  "README.md",
  "SECURITY.md",
  "package.json",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
  "rust-toolchain.toml",
  "CONTRIBUTING.md",
  "docs/engineering.md",
];
async function inventory(cwd) {
  const tree = (await successful("git", ["write-tree"], { cwd })).trim();
  return sourceTree(await successful("git", ["ls-tree", "-rz", tree], { cwd }));
}
async function archive(cwd) {
  const tree = (await successful("git", ["write-tree"], { cwd })).trim();
  const bytes = await successful("git", ["archive", "--format=tar", tree], {
    cwd,
    encoding: "buffer",
  });
  return successful("tar", ["-tf", "-"], { input: bytes });
}
async function verify(cwd) {
  await successful("git", ["init", "--quiet", "--template="], { cwd });
  await mkdir(join(cwd, "docs"));
  for (const path of required)
    await writeFile(join(cwd, path), "public fixture\n");
  await successful("git", ["add", "--all"], { cwd });
  const paths = await inventory(cwd);
  assert.equal(archivePaths(paths, await archive(cwd)), required.length);
  await writeFile(join(cwd, ".gitattributes"), "SECURITY.md export-ignore\n");
  await successful("git", ["add", ".gitattributes"], { cwd });
  const incomplete = await archive(cwd);
  assert.throws(() => archivePaths(paths, incomplete));
  await writeFile(join(cwd, ".env"), "private fixture\n");
  await successful("git", ["add", "--force", ".env"], { cwd });
  await assert.rejects(inventory(cwd));
  assert.equal((await execute(join(cwd, "missing-command"), [])).code, null);
  assert.equal(
    (await execute(process.execPath, ["-e", "process.exit(3)"])).code,
    3,
  );
  assert.equal(
    (
      await execute(process.execPath, ["-e", "setTimeout(()=>{},10000)"], {
        timeout: 50,
      })
    ).code,
    null,
  );
}
const cwd = await mkdtemp(join(tmpdir(), "darkhorse-release-tests-"));
try {
  await verify(cwd);
  console.log(
    "Release tooling integration passed: archive completeness, private-file rejection, missing executable, failed process and deadline.",
  );
} finally {
  await rm(cwd, { recursive: true, force: true });
}
