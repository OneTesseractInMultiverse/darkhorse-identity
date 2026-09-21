import test from "node:test";
import assert from "node:assert/strict";
import {
  sourceTree,
  archivePaths,
  sourceReference,
  objectId,
} from "../../lib/source-package.mjs";
test("source references cannot inject options or conceal an invalid Git object", () => {
  assert.equal(sourceReference([]), "HEAD");
  assert.equal(sourceReference(["HEAD~1"]), "HEAD~1");
  for (const args of [
    [""],
    ["-o/tmp/output"],
    ["HEAD", "main"],
    ["a".repeat(257)],
    ["HEAD\nother"],
    ["HEAD main"],
  ])
    assert.throws(() => sourceReference(args));
  assert.equal(objectId("a".repeat(40) + "\n"), "a".repeat(40));
  for (const raw of [
    "",
    "a".repeat(39),
    "g".repeat(40),
    "a".repeat(40) + "\nextra",
  ])
    assert.throws(() => objectId(raw));
});
const row = (path, mode = "100644") =>
  `${mode} blob ${"a".repeat(40)}\t${path}\0`;
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
const valid = () => required.map((p) => row(p)).join("");
test("a portable committed source tree and identical exported inventory pass", () => {
  const paths = sourceTree(valid() + row("scripts/check.mjs", "100755"));
  assert.equal(paths.length, required.length + 1);
  assert.equal(
    archivePaths(paths, ["docs/", ...paths].join("\n") + "\n"),
    paths.length,
  );
});
test("private state, credentials, dependency outputs and escaping paths cannot enter a source package", () => {
  for (const path of [
    ".context/plan.md",
    "context/plan.md",
    ".local/key",
    "apps/console/node_modules/a",
    "target/a",
    ".idea/a",
    "apps/console/build/a",
    ".env",
    "config/.env.production",
    "key.pem",
    "cert.p12",
    "secret.key",
    "../escape",
    "a/../../escape",
    "/absolute",
    "a\\b",
    "a\nb",
    "a//b",
    "a/./b",
  ]) {
    assert.throws(() => sourceTree(valid() + row(path)), path);
  }
  assert.doesNotThrow(() => sourceTree(valid() + row(".env.example")));
});
test("links, submodules, duplicates, malformed entries and omitted public source fail", () => {
  for (const extra of [
    row("link", "120000"),
    row("module", "160000"),
    row("Cargo.lock"),
    "bad\0",
    row("a").slice(0, -1),
  ])
    assert.throws(() => sourceTree(valid() + extra));
  assert.throws(() => sourceTree(row("README.md")));
  const paths = sourceTree(valid());
  for (const list of [
    paths.slice(1),
    [...paths, ".context/a"],
    [...paths, paths[0]],
  ])
    assert.throws(() => archivePaths(paths, list.join("\n") + "\n"));
});
