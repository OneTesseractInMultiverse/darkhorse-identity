import { resolve } from "node:path";
import { createHash } from "node:crypto";
import { successful } from "./lib/security-command.mjs";
import {
  sourceTree,
  archivePaths,
  sourceReference,
  objectId,
} from "./lib/source-package.mjs";
process.chdir(resolve(import.meta.dirname, ".."));
function evidence(revision, files, archive) {
  return JSON.stringify(
    {
      version: 1,
      revision,
      files,
      archiveSha256: createHash("sha256").update(archive).digest("hex"),
      status: "pass",
      scope:
        "source inventory only; not secret scanning, license approval or production qualification",
    },
    null,
    2,
  );
}
async function main() {
  const ref = sourceReference(process.argv.slice(2));
  const revision = objectId(
    await successful("git", [
      "rev-parse",
      "--verify",
      "--end-of-options",
      `${ref}^{object}`,
    ]),
  );
  const paths = sourceTree(
    await successful("git", ["ls-tree", "-r", "-z", "--full-tree", revision]),
  );
  const archive = await successful(
    "git",
    ["archive", "--format=tar", revision],
    { encoding: "buffer" },
  );
  const files = archivePaths(
    paths,
    await successful("tar", ["-tf", "-"], { input: archive }),
  );
  console.log(evidence(revision, files, archive));
}
main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
