export const ownerLabel = "org.darkhorse.boundary-run";
export function boundarySuite(args) {
  if (args.length !== 1 || !["postgres", "redis"].includes(args[0]))
    throw new Error("Choose exactly one boundary suite: postgres or redis.");
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
  const matches = [
    ...result.stdout.matchAll(
      /^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out; finished in [\d.]+s$/gm,
    ),
  ];
  const suites = matches.slice(0, 3).map((m) => ({
    passed: Number(m[2]),
    failed: Number(m[3]),
    ignored: Number(m[4]),
    measured: Number(m[5]),
    filtered: Number(m[6]),
  }));
  const worker =
    /^test multiprocess_worker \.\.\. ignored, executed by the separate-process parent scenario with disposable infrastructure$/m.test(
      result.stdout,
    ) &&
    /^test separate_processes_share_one_budget_without_shared_connection_pools \.\.\. ok$/m.test(
      result.stdout,
    );
  const counts =
    suites.length === (suite === "postgres" ? 1 : 2) &&
    suites.every(
      (s, i) =>
        s.passed > 0 &&
        s.failed === 0 &&
        s.measured === 0 &&
        s.filtered === 0 &&
        s.ignored === (suite === "redis" && i === 1 ? 1 : 0),
    );
  const passed =
    result.code === 0 &&
    !result.interrupted &&
    !result.overflow &&
    counts &&
    matches.every((m) => m[1] === "ok") &&
    (suite === "postgres" || worker);
  return {
    status: passed ? "passed" : "failed",
    exitCode: result.code,
    interrupted: result.interrupted,
    outputLimitExceeded: result.overflow,
    suites,
    worker:
      suite === "redis" && worker
        ? "executed by passing parent scenario"
        : "not applicable or not verified",
    failedTests: [
      ...result.stdout.matchAll(
        /^test ([a-zA-Z0-9_:]{1,200}) \.\.\. FAILED$/gm,
      ),
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
