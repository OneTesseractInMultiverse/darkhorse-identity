import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { ciWorkflowPolicyFindings } from "./lib/ci-workflow-policy.mjs";

const workflowPath = resolve(".github/workflows/ci.yaml");
const source = await readFile(workflowPath, "utf8");
const findings = ciWorkflowPolicyFindings(source);

if (findings.length > 0) {
  for (const { code, message } of findings) {
    process.stderr.write(`CI workflow policy ${code}: ${message}\n`);
  }
  process.exitCode = 1;
} else {
  process.stdout.write(
    "CI workflow policy passed: read-only, bounded, pinned, and secret-free.\n",
  );
}
