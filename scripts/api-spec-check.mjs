import { readFile, lstat } from "node:fs/promises";
import { readBoundedApiInput, validateApiReference } from "./lib/api-spec.mjs";

async function source(path) {
  const text = await readBoundedApiInput(path, { readFile, lstat });
  return text;
}

try {
  const [
    specificationText,
    classificationText,
    cargoText,
    bundledSpec,
    bundledClassification,
  ] = await Promise.all([
    source("docs/api/openapi-v1.json"),
    source("docs/api/route-classification-v1.json"),
    source("Cargo.toml"),
    source("apps/console/static/reference/openapi-v1.json"),
    source("apps/console/static/reference/route-classification-v1.json"),
  ]);
  let specification;
  let classification;
  try {
    specification = JSON.parse(specificationText);
    classification = JSON.parse(classificationText);
  } catch {
    throw new Error("API reference input is not valid JSON.");
  }
  const releaseVersion = cargoText.match(
    /^version\s*=\s*"([^"\r\n]+)"\s*$/m,
  )?.[1];
  const result = validateApiReference({
    specification,
    classification,
    releaseVersion,
  });
  if (!result.ok) throw new Error(result.error);
  if (
    bundledSpec !== specificationText ||
    bundledClassification !== classificationText
  )
    throw new Error(
      "Bundled API reference assets are stale; run make api-reference-bundle.",
    );
  process.stdout.write(
    `API contract check passed (${result.operationCount} supported integration operations).\n`,
  );
} catch (error) {
  process.stderr.write(
    `API contract check failed: ${error instanceof Error ? error.message : "invalid input"}\n`,
  );
  process.exitCode = 1;
}
