import assert from "node:assert/strict";
import test from "node:test";
import { ciWorkflowPolicyFindings } from "../../lib/ci-workflow-policy.mjs";

const checkout = "a".repeat(40);
const upload = "b".repeat(40);
const workflow = (changes = []) => {
  let source = `name: CI
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  workflow_dispatch:
permissions:
  contents: read
jobs:
  source:
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@${checkout}
        with:
          persist-credentials: false
      - uses: actions/upload-artifact@${upload}
        if: \${{ always() }}
        with:
          name: security-report
          path: .local/security/report.json
          if-no-files-found: error
          retention-days: 14
  boundaries:
    timeout-minutes: \${{ matrix.job_minutes }}
    strategy:
      matrix:
        job_minutes: 40
        suite_minutes: 27
    steps:
      - uses: actions/checkout@${checkout}
        with:
          persist-credentials: false
      - run: make test-postgres
        timeout-minutes: \${{ matrix.suite_minutes }}
      - uses: actions/upload-artifact@${upload}
        if: \${{ always() }}
        with:
          name: boundary-report
          path: .local/ci-boundary/\${{ matrix.suite }}/report.json
          if-no-files-found: error
          retention-days: 14
`;
  for (const [before, after] of changes) source = source.replace(before, after);
  return source;
};

const codes = (source) =>
  ciWorkflowPolicyFindings(source).map(({ code }) => code);

test("accepts read-only, pinned, bounded workflows with redacted report artifacts", () => {
  assert.deepEqual(ciWorkflowPolicyFindings(workflow()), []);
});

test("requires an ordinary pull request trigger and rejects privileged triggers", () => {
  assert.ok(
    codes(workflow([["  pull_request:\n    branches: [main]\n", ""]])).includes(
      "pull-request-trigger-missing",
    ),
  );
  assert.ok(
    codes(workflow([["pull_request:", "pull_request_target:"]])).includes(
      "pull-request-target",
    ),
  );
});

test("rejects write permissions and secret references", () => {
  assert.ok(
    codes(workflow([["contents: read", "contents: write"]])).includes(
      "permissions-not-read-only",
    ),
  );
  assert.ok(
    codes(
      workflow([["name: CI", "name: CI\n# ${{ secrets.DEPLOY_KEY }}"]]),
    ).includes("secret-reference"),
  );
});

test("requires pinned actions and checkout credentials to remain disabled", () => {
  assert.ok(
    codes(
      workflow([[`actions/checkout@${checkout}`, "actions/checkout@v7"]]),
    ).includes("action-not-pinned"),
  );
  assert.ok(
    codes(
      workflow([["persist-credentials: false", "persist-credentials: true"]]),
    ).includes("checkout-credentials-persisted"),
  );
  assert.ok(
    codes(workflow([["          persist-credentials: false\n", ""]])).includes(
      "checkout-credentials-persisted",
    ),
  );
});

test("rejects caches and artifact paths outside the bounded report allowlist", () => {
  assert.ok(
    codes(
      workflow([
        [
          "      - run: make test-postgres",
          "      - uses: actions/cache@cccccccccccccccccccccccccccccccccccccccc\n      - run: make test-postgres",
        ],
      ]),
    ).includes("dependency-cache-enabled"),
  );
  assert.ok(
    codes(
      workflow([
        [
          "      - run: make test-postgres",
          `      - uses: actions/cache/restore@${"c".repeat(40)}\n      - run: make test-postgres`,
        ],
      ]),
    ).includes("dependency-cache-enabled"),
  );
  assert.ok(
    codes(
      workflow([
        ["path: .local/security/report.json", "path: .local/security/logs/**"],
      ]),
    ).includes("artifact-path-not-allowlisted"),
  );
  assert.ok(
    codes(workflow([["retention-days: 14", "retention-days: 90"]])).includes(
      "artifact-retention-unbounded",
    ),
  );
  assert.ok(
    codes(
      workflow([["if-no-files-found: error", "if-no-files-found: warn"]]),
    ).includes("artifact-missing-file-not-fatal"),
  );
});

test("requires every job and boundary suite timeout to stay within sixty minutes", () => {
  assert.ok(
    codes(workflow([["    timeout-minutes: 45\n", ""]])).includes(
      "job-timeout-unbounded",
    ),
  );
  assert.ok(
    codes(workflow([["timeout-minutes: 45", "timeout-minutes: 61"]])).includes(
      "job-timeout-unbounded",
    ),
  );
  assert.ok(
    codes(workflow([["suite_minutes: 27", "suite_minutes: 61"]])).includes(
      "suite-timeout-unbounded",
    ),
  );
});
