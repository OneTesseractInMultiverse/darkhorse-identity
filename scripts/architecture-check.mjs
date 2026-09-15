import { execFileSync } from "node:child_process";
import { readdir } from "node:fs/promises";
import { boundaryErrors } from "./lib/architecture.mjs";

const metadata = JSON.parse(
  execFileSync(
    "cargo",
    ["metadata", "--offline", "--locked", "--no-deps", "--format-version", "1"],
    { encoding: "utf8" },
  ),
);
const paths = (await readdir("apps/console/src", { recursive: true })).map(
  (path) => `apps/console/src/${path}`,
);
const errors = boundaryErrors(metadata.packages, paths);
if (errors.length) {
  console.error(errors.join("\n"));
  process.exitCode = 1;
} else console.log("Architecture boundaries passed.");
