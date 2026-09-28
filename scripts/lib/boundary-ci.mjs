import { browserEvidence } from "./browser-evidence.mjs";
export const ownerLabel = "org.darkhorse.boundary-run";
export function boundarySuite(args) {
  if (args.length !== 1 || !["postgres", "redis", "browser"].includes(args[0]))
    throw new Error(
      "Choose exactly one boundary suite: postgres, redis or browser.",
    );
  return args[0];
}
export function resourceLabels(owner) {
  if (owner === undefined) return [];
  if (!/^[a-f0-9]{32}$/.test(owner)) throw new Error("Invalid boundary owner.");
  return ["--label", `${ownerLabel}=${owner}`];
}
export function resourceIds(output) {
  const ids = output.trim() ? output.trim().split("\n") : [];
  if (ids.length > 100 || ids.some((id) => !/^[a-f0-9]{64}$/.test(id)))
    throw new Error("Invalid or excessive owned Docker resource inventory.");
  return [...new Set(ids)];
}
export function boundaryResult(suite, result) {
  const testOutput = `${result.stdout}\n${result.stderr ?? ""}`;
  const matches = [
    ...testOutput.matchAll(
      /^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out; finished in [\d.]+s$/gm,
    ),
  ];
  const suites = matches.slice(0, 5).map((m) => ({
    passed: Number(m[2]),
    failed: Number(m[3]),
    ignored: Number(m[4]),
    measured: Number(m[5]),
    filtered: Number(m[6]),
  }));
  const outageTests = [
    ...testOutput.matchAll(
      /^test (?:[a-zA-Z0-9_]+::)*redis_cache_outage_keeps_postgres_introspection_authoritative \.\.\. ok$/gm,
    ),
  ];
  const workerRecords = [
      ...testOutput.matchAll(
        /^test multiprocess_worker \.\.\. ignored, executed by the separate-process parent scenario with disposable infrastructure$/gm,
      ),
    ],
    parentRecords = [
      ...testOutput.matchAll(
        /^test separate_processes_share_one_budget_without_shared_connection_pools \.\.\. ok$/gm,
      ),
    ];
  const worker = workerRecords.length === 1 && parentRecords.length === 1;
  const expectedSuites = { postgres: 1, redis: 3, browser: 5 }[suite];
  const isolatedOutage = suite !== "postgres" && outageTests.length === 1;
  const counts =
    matches.length === expectedSuites &&
    suites.length === expectedSuites &&
    suites.every((s) => s.passed > 0 && s.failed === 0 && s.measured === 0) &&
    (suite === "postgres"
      ? suites.every((s) => s.ignored === 0 && s.filtered === 0)
      : isolatedOutage &&
        suites.filter((s) => s.ignored === 1 && s.filtered === 0).length ===
          1 &&
        suites.filter((s) => s.ignored > 0).length === 1 &&
        suites.filter((s) => s.filtered > 0).length === 1 &&
        suites.filter(
          (s) => s.passed === 1 && s.ignored === 0 && s.filtered > 0,
        ).length === 1);
  const browser =
    suite === "browser" ? browserEvidence(result.stdout) : undefined;
  const passed =
    result.code === 0 &&
    !result.interrupted &&
    !result.overflow &&
    counts &&
    matches.every((m) => m[1] === "ok") &&
    (suite === "postgres" || worker) &&
    (!browser || browser.status === "completed");
  return {
    status: passed ? "passed" : "failed",
    exitCode: result.code,
    interrupted: result.interrupted,
    outputLimitExceeded: result.overflow,
    suites,
    ...(browser ? { browser } : {}),
    worker:
      suite !== "postgres" && worker
        ? "executed by passing parent scenario"
        : "not applicable or not verified",
    failedTests: [
      ...testOutput.matchAll(/^test ([a-zA-Z0-9_:]{1,200}) \.\.\. FAILED$/gm),
    ]
      .slice(0, 100)
      .map((m) => m[1]),
  };
}

export function toolVersions(raw) {
  const rust = /^rustc (\d+\.\d+\.\d+)\b/.exec(raw.rust);
  const docker = /^(\d+\.\d+\.\d+) \/ (\d+\.\d+\.\d+)\s*$/.exec(raw.docker);
  const openssl = /^(OpenSSL|LibreSSL) (\d+\.\d+\.\d+[a-z]?)\b/.exec(
    raw.openssl,
  );
  if (!rust || !docker || !openssl)
    throw new Error("Unrecognized boundary tool versions.");
  return {
    rust: rust[1],
    dockerClient: docker[1],
    dockerServer: docker[2],
    openssl: `${openssl[1]} ${openssl[2]}`,
  };
}
