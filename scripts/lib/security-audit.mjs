const severities = ["info", "low", "moderate", "high", "critical"];
function requireValue(ok) {
  if (!ok)
    throw new Error("Incomplete, filtered or inconsistent advisory report.");
}
const object = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const count = (value) => Number.isSafeInteger(value) && value >= 0;
function name(value) {
  requireValue(
    typeof value === "string" &&
      value.length > 0 &&
      value.length <= 200 &&
      !/[\x00-\x1f\x7f]/.test(value),
  );
  return value;
}
function rustFinding(value, kind) {
  requireValue(object(value) && object(value.package));
  const id = value.advisory?.id ?? (kind === "yanked" ? "yanked" : null);
  requireValue(id === "yanked" || /^RUSTSEC-\d{4}-\d{4}$/.test(id));
  return {
    id,
    package: name(value.package.name),
    version: name(value.package.version),
    kind,
  };
}
export function rustReport(value) {
  requireValue(
    object(value) &&
      object(value.database) &&
      object(value.lockfile) &&
      object(value.settings),
  );
  const settings = value.settings;
  requireValue(
    [settings.ignore, settings.target_arch, settings.target_os].every(
      (v) => Array.isArray(v) && v.length === 0,
    ) && settings.severity === null,
  );
  requireValue(
    Array.isArray(settings.informational_warnings) &&
      ["unmaintained", "unsound", "notice"].every((k) =>
        settings.informational_warnings.includes(k),
      ),
  );
  requireValue(
    count(value.database["advisory-count"]) &&
      value.database["advisory-count"] > 0 &&
      /^[a-f0-9]{40}$/.test(value.database["last-commit"]),
  );
  requireValue(
    Number.isFinite(Date.parse(value.database["last-updated"])) &&
      count(value.lockfile["dependency-count"]) &&
      value.lockfile["dependency-count"] > 0,
  );
  const v = value.vulnerabilities;
  requireValue(
    object(v) &&
      count(v.count) &&
      Array.isArray(v.list) &&
      v.count === v.list.length &&
      v.found === v.count > 0 &&
      object(value.warnings),
  );
  const findings = v.list.map((item) => rustFinding(item, "vulnerability"));
  for (const [kind, items] of Object.entries(value.warnings)) {
    requireValue(
      ["unmaintained", "unsound", "notice", "yanked"].includes(kind) &&
        Array.isArray(items),
    );
    findings.push(...items.map((item) => rustFinding(item, kind)));
  }
  return {
    dependencies: value.lockfile["dependency-count"],
    databaseRevision: value.database["last-commit"],
    databaseUpdated: value.database["last-updated"],
    findings,
  };
}
export function nodeReport(value) {
  requireValue(
    object(value) &&
      object(value.advisories) &&
      object(value.metadata) &&
      object(value.metadata.vulnerabilities),
  );
  const counts = value.metadata.vulnerabilities;
  requireValue(
    Object.keys(counts).length === severities.length &&
      severities.every((k) => count(counts[k])),
  );
  requireValue(
    count(value.metadata.totalDependencies) &&
      value.metadata.totalDependencies > 0,
  );
  const findings = Object.values(value.advisories).map((v) => {
    requireValue(
      object(v) &&
        /^GHSA-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}$/.test(
          v.github_advisory_id,
        ) &&
        severities.includes(v.severity),
    );
    return {
      id: v.github_advisory_id,
      package: name(v.module_name),
      kind: v.severity,
    };
  });
  for (const severity of severities)
    requireValue(
      counts[severity] === findings.filter((f) => f.kind === severity).length,
    );
  return { dependencies: value.metadata.totalDependencies, findings };
}
export function outcome(report, code) {
  if (code !== 0 && code !== 1) return "error";
  if (report.findings.length) return "blocked";
  return code === 0 ? "pass" : "error";
}
export function qualificationStatus(...reports) {
  if (
    reports.length === 0 ||
    reports.some((report) => report.status === "error")
  )
    return "error";
  return reports.every((report) => report.status === "pass")
    ? "pass"
    : "blocked";
}
export function inspectScan(kind, raw, code) {
  try {
    requireValue(kind === "rust" || kind === "node");
    const report = (kind === "rust" ? rustReport : nodeReport)(JSON.parse(raw));
    return { ...report, exitCode: code, status: outcome(report, code) };
  } catch {
    return {
      status: "error",
      exitCode: code,
      reason:
        "Scanner output was unavailable, filtered, malformed or inconsistent.",
    };
  }
}
export function verifyTools(rust, node) {
  requireValue(
    /^cargo-audit(?:-audit)? 0\.22\.2\s*$/.test(rust) &&
      node.trim() === "11.19.0",
  );
  return { cargoAudit: "0.22.2", pnpm: "11.19.0" };
}
