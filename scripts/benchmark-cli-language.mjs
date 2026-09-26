// One-shot process startup and static help only; no services or authority claim.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
import { cpus, platform, arch } from "node:os";
import { resolve } from "node:path";
import { performance } from "node:perf_hooks";

const [baseline, localized] = process.argv
  .slice(2)
  .map((file) => resolve(file));
assert.ok(
  baseline && localized,
  "Supply baseline and localized release binaries.",
);
assert.notEqual(baseline, localized);
const env = { PATH: process.env.PATH, LC_ALL: "C", NO_COLOR: "1" };
const variants = [
  { name: "baseline-en", binary: baseline, args: ["--help"], label: "Usage:" },
  {
    name: "localized-en",
    binary: localized,
    args: ["--locale", "en", "--help"],
    label: "Usage:",
  },
  {
    name: "localized-es",
    binary: localized,
    args: ["--locale", "es", "--help"],
    label: "Uso:",
  },
];
function run(variant) {
  const started = performance.now();
  const output = execFileSync(variant.binary, variant.args, {
    env,
    timeout: 5000,
    maxBuffer: 65536,
    encoding: "utf8",
  });
  const durationMs = performance.now() - started;
  assert.ok(output.includes(variant.label));
  assert.ok(!output.includes("\x1b"));
  return { durationMs, outputBytes: Buffer.byteLength(output) };
}
const observations = [];
for (const variant of variants) for (let n = 0; n < 10; n++) run(variant);
for (let repeat = 0; repeat < 5; repeat++) {
  for (let offset = 0; offset < variants.length; offset++) {
    const variant = variants[(offset + repeat) % variants.length];
    for (let sample = 0; sample < 25; sample++)
      observations.push({
        repeat,
        sample,
        variant: variant.name,
        ...run(variant),
      });
  }
}
const hash = (file) =>
  createHash("sha256").update(readFileSync(file)).digest("hex");
const inputs = [
  "Cargo.lock",
  "apps/server/src/main.rs",
  "crates/adapters/src/operator/cli.rs",
  "crates/adapters/src/operator/cli/help.rs",
  "crates/adapters/src/operator/cli/tree.rs",
  "crates/adapters/src/operator/localization.rs",
  "crates/adapters/src/operator/localization/messages.rs",
];
console.log(
  JSON.stringify(
    {
      schema: 1,
      date: new Date().toISOString(),
      scope:
        "Sequential release-binary startup and fixed root help, including process creation and output capture. Warm filesystem cache, no CPU isolation. No SQL, Redis, credential checks, concurrent work or production capacity claim.",
      host: { platform: platform(), arch: arch(), cpu: cpus()[0]?.model },
      node: process.version,
      inputs: Object.fromEntries(inputs.map((path) => [path, hash(path)])),
      binaries: [
        { name: "baseline", file: baseline },
        { name: "localized", file: localized },
      ].map(({ name, file }) => ({
        name,
        bytes: statSync(file).size,
        sha256: hash(file),
      })),
      warmupPerVariant: 10,
      observations,
    },
    null,
    2,
  ),
);
