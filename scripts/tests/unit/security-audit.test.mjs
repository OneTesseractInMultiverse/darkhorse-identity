import test from "node:test";
import assert from "node:assert/strict";
import {
  rustReport,
  nodeReport,
  outcome,
  inspectScan,
  verifyTools,
  qualificationStatus,
} from "../../lib/security-audit.mjs";
test("qualification requires every scan to pass and preserves incomplete evidence", () => {
  assert.equal(qualificationStatus(), "error");
  assert.equal(
    qualificationStatus({ status: "pass" }, { status: "pass" }),
    "pass",
  );
  assert.equal(
    qualificationStatus({ status: "pass" }, { status: "blocked" }),
    "blocked",
  );
  assert.equal(
    qualificationStatus({ status: "blocked" }, { status: "error" }),
    "error",
  );
});
const rust = () => ({
  database: {
    "advisory-count": 12,
    "last-commit": "a".repeat(40),
    "last-updated": "2026-09-21T00:00:00Z",
  },
  lockfile: { "dependency-count": 10 },
  settings: {
    target_arch: [],
    target_os: [],
    severity: null,
    ignore: [],
    informational_warnings: ["unmaintained", "unsound", "notice"],
  },
  vulnerabilities: { found: false, count: 0, list: [] },
  warnings: {},
});
const npm = () => ({
  advisories: {},
  metadata: {
    vulnerabilities: { info: 0, low: 0, moderate: 0, high: 0, critical: 0 },
    totalDependencies: 12,
  },
});
test("only coherent complete reports with successful processes can pass", () => {
  assert.equal(outcome(rustReport(rust()), 0), "pass");
  assert.equal(outcome(nodeReport(npm()), 0), "pass");
  for (const code of [1, 2, null])
    assert.equal(outcome(rustReport(rust()), code), "error");
});
test("process failures and parser errors produce explicit errors without leaking raw scanner output", () => {
  assert.equal(inspectScan("rust", JSON.stringify(rust()), 0).status, "pass");
  assert.equal(inspectScan("node", JSON.stringify(npm()), 2).status, "error");
  for (const raw of [
    "",
    "private diagnostic",
    "{}",
    JSON.stringify({ error: "registry failure" }),
  ]) {
    const result = inspectScan("node", raw, null);
    assert.equal(result.status, "error");
    assert.equal(JSON.stringify(result).includes("private diagnostic"), false);
  }
  assert.equal(
    inspectScan("unknown", JSON.stringify(npm()), 0).status,
    "error",
  );
  const r = rust();
  r.vulnerabilities = {
    found: true,
    count: 1,
    list: [
      {
        advisory: { id: "RUSTSEC-2024-0001" },
        package: { name: "example", version: "1.0.0" },
      },
    ],
  };
  assert.equal(inspectScan("rust", JSON.stringify(r), 1).status, "blocked");
  r.warnings.unknown = [];
  assert.equal(inspectScan("rust", JSON.stringify(r), 0).status, "error");
});
test("auditor versions are explicit and incompatible report producers are rejected", () => {
  assert.deepEqual(verifyTools("cargo-audit-audit 0.22.2\n", "11.19.0\n"), {
    cargoAudit: "0.22.2",
    pnpm: "11.19.0",
  });
  assert.doesNotThrow(() => verifyTools("cargo-audit 0.22.2", "11.19.0"));
  for (const [r, n] of [
    ["0.22.2", "11.19.0"],
    ["cargo-audit 0.23.0", "11.19.0"],
    ["cargo-audit 0.22.2", "12.0.0"],
  ])
    assert.throws(() => verifyTools(r, n));
});
test("vulnerabilities and informational warnings are retained as blockers", () => {
  const r = rust();
  r.warnings = {
    unmaintained: [
      {
        advisory: { id: "RUSTSEC-2023-0089" },
        package: { name: "atomic-polyfill", version: "1.0.3" },
      },
    ],
  };
  assert.deepEqual(rustReport(r).findings, [
    {
      id: "RUSTSEC-2023-0089",
      package: "atomic-polyfill",
      version: "1.0.3",
      kind: "unmaintained",
    },
  ]);
  assert.equal(outcome(rustReport(r), 1), "blocked");
  assert.equal(outcome(rustReport(r), 0), "blocked");
  const n = npm();
  n.advisories = {
    one: {
      github_advisory_id: "GHSA-pxg6-pf52-xh8x",
      module_name: "cookie",
      severity: "low",
    },
  };
  n.metadata.vulnerabilities.low = 1;
  assert.equal(outcome(nodeReport(n), 1), "blocked");
});
test("missing, malformed, filtered and contradictory scan results never pass", () => {
  for (const value of [
    null,
    {},
    [],
    { error: "offline" },
    { ...rust(), warnings: null },
  ])
    assert.throws(() => rustReport(value));
  for (const change of [
    { ignore: ["RUSTSEC-2023-0089"] },
    { target_arch: ["x86_64"] },
    { target_os: ["linux"] },
    { severity: 5 },
    { informational_warnings: [] },
  ])
    assert.throws(() =>
      rustReport({ ...rust(), settings: { ...rust().settings, ...change } }),
    );
  for (const vulnerabilities of [
    { found: true, count: 0, list: [] },
    { found: false, count: 1, list: [] },
    { found: false, count: -1, list: [] },
  ])
    assert.throws(() => rustReport({ ...rust(), vulnerabilities }));
  for (const value of [
    null,
    {},
    [],
    { error: "registry offline" },
    { ...npm(), advisories: null },
    { ...npm(), metadata: { vulnerabilities: { high: 0 } } },
  ])
    assert.throws(() => nodeReport(value));
  const contradicted = npm();
  contradicted.metadata.vulnerabilities.high = 1;
  assert.throws(() => nodeReport(contradicted));
  const invalid = npm();
  invalid.metadata.vulnerabilities.unknown = 0;
  assert.throws(() => nodeReport(invalid));
});
