import { mkdir, open, readFile, lstat, rename, rm } from "node:fs/promises";
import { join } from "node:path";
import { randomBytes } from "node:crypto";
import { readBoundedApiInput, validateApiReference } from "./lib/api-spec.mjs";

const mode = process.argv[2] ?? "generate";
const files = [
  ["docs/api/openapi-v1.json", "apps/console/static/reference/openapi-v1.json"],
  [
    "docs/api/route-classification-v1.json",
    "apps/console/static/reference/route-classification-v1.json",
  ],
];

async function read(path) {
  return readBoundedApiInput(path, { readFile, lstat });
}

try {
  if (mode !== "generate" && mode !== "check")
    throw new Error("Use generate or check.");
  const [specText, classificationText, cargo] = await Promise.all([
    read(files[0][0]),
    read(files[1][0]),
    read("Cargo.toml"),
  ]);
  const releaseVersion = cargo.match(
    /\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"\r\n]+)"\s*$/m,
  )?.[1];
  let specification;
  let classification;
  try {
    specification = JSON.parse(specText);
    classification = JSON.parse(classificationText);
  } catch {
    throw new Error("API reference source is not valid JSON.");
  }
  const validation = validateApiReference({
    specification,
    classification,
    releaseVersion,
  });
  if (!validation.ok) throw new Error(validation.error);
  const outputs = [specText, classificationText];
  if (mode === "check") {
    for (let index = 0; index < files.length; index++) {
      if ((await read(files[index][1])) !== outputs[index])
        throw new Error(
          "Bundled API reference is stale; run make api-reference-bundle.",
        );
    }
    process.stdout.write(
      "Bundled API reference assets match their reviewed sources.\n",
    );
  } else {
    const destination = "apps/console/static/reference";
    await mkdir(destination, { recursive: true });
    const pending = [];
    try {
      for (let index = 0; index < files.length; index++) {
        const output = join(
          destination,
          `.reference-${randomBytes(8).toString("hex")}.tmp`,
        );
        const handle = await open(output, "wx", 0o600);
        pending.push([output, files[index][1]]);
        try {
          await handle.writeFile(outputs[index], "utf8");
          await handle.sync();
          await handle.chmod(0o644);
        } finally {
          await handle.close();
        }
      }
      for (const [temporary, final] of pending) await rename(temporary, final);
    } finally {
      await Promise.all(
        pending.map(([temporary]) => rm(temporary, { force: true })),
      );
    }
    process.stdout.write(
      `Bundled ${validation.operationCount} API operations for release ${releaseVersion}.\n`,
    );
  }
} catch (error) {
  process.stderr.write(
    `API reference bundle ${mode} failed: ${error instanceof Error ? error.message : "invalid input"}\n`,
  );
  process.exitCode = 1;
}
