const MAX_JOB_MINUTES = 60;
const MAX_ARTIFACT_DAYS = 14;
const ALLOWED_ARTIFACT_PATHS = new Set([
  ".local/security/report.json",
  ".local/ci-boundary/${{ matrix.suite }}/report.json",
]);

function topLevelSection(source, name) {
  const lines = source.split(/\r?\n/);
  const start = lines.findIndex((line) => line === `${name}:`);
  if (start < 0) return undefined;
  const body = [];
  for (let index = start + 1; index < lines.length; index += 1) {
    const line = lines[index];
    if (line.trim() && !/^\s/.test(line)) break;
    body.push(line);
  }
  return body.join("\n");
}

function listStepBlocks(source, action) {
  const lines = source.split(/\r?\n/);
  const blocks = [];
  for (let index = 0; index < lines.length; index += 1) {
    if (!/^ {6}- /.test(lines[index])) continue;
    const block = [lines[index]];
    for (let cursor = index + 1; cursor < lines.length; cursor += 1) {
      if (
        /^ {6}- /.test(lines[cursor]) ||
        (/^\S/.test(lines[cursor]) && lines[cursor].trim())
      ) {
        break;
      }
      block.push(lines[cursor]);
    }
    const candidate = block.join("\n");
    if (candidate.includes(action)) blocks.push(candidate);
  }
  return blocks;
}

function findingsForTriggers(source, findings) {
  if (/^\s*pull_request_target\s*:/m.test(source)) {
    findings.push({
      code: "pull-request-target",
      message: "Do not run untrusted changes with pull_request_target.",
    });
  }
  const triggers = topLevelSection(source, "on") ?? "";
  for (const trigger of ["push", "pull_request"]) {
    const pattern = new RegExp(
      `^  ${trigger}:\\n    branches: \\[main\\]$`,
      "m",
    );
    if (!pattern.test(triggers)) {
      findings.push({
        code: "pull-request-trigger-missing",
        message: "Push and pull-request verification must target main.",
      });
      break;
    }
  }
}

function findingsForPermissions(source, findings) {
  const permissions = topLevelSection(source, "permissions") ?? "";
  const entries = permissions
    .split(/\r?\n/)
    .map((line) => /^ {2}([a-z-]+):\s*([a-z-]+)\s*$/.exec(line))
    .filter(Boolean)
    .map((match) => [match[1], match[2]]);
  if (
    entries.length !== 1 ||
    entries[0][0] !== "contents" ||
    entries[0][1] !== "read" ||
    /^\s*[a-z-]+:\s*write(?:-all)?\s*$/m.test(source)
  ) {
    findings.push({
      code: "permissions-not-read-only",
      message: "Hosted verification must use read-only repository permissions.",
    });
  }
  if (/\bsecrets\.[A-Z0-9_]+/i.test(source)) {
    findings.push({
      code: "secret-reference",
      message: "Hosted verification must not reference repository secrets.",
    });
  }
}

function findingsForActions(source, findings) {
  const actionLines = source
    .split(/\r?\n/)
    .filter((line) => /^\s*(?:-\s+)?uses:\s*/.test(line));
  if (
    actionLines.length === 0 ||
    actionLines.some((line) => !/@[a-f0-9]{40}(?:\s|$)/.test(line))
  ) {
    findings.push({
      code: "action-not-pinned",
      message: "Every hosted action must use an immutable commit reference.",
    });
  }
  if (
    /uses:\s*actions\/cache(?:\/[a-z-]+)?@|^\s*cache:\s*(?!false\s*(?:#.*)?$)/m.test(
      source,
    )
  ) {
    findings.push({
      code: "dependency-cache-enabled",
      message: "Hosted verification must not use dependency caches.",
    });
  }

  const checkouts = listStepBlocks(source, "actions/checkout@");
  if (
    checkouts.length === 0 ||
    checkouts.some(
      (block) => !/^\s{10}persist-credentials:\s*false\s*$/m.test(block),
    )
  ) {
    findings.push({
      code: "checkout-credentials-persisted",
      message: "Checkout must not persist its repository token.",
    });
  }
}

function findingsForArtifacts(source, findings) {
  const artifacts = listStepBlocks(source, "actions/upload-artifact@");
  if (artifacts.length === 0) {
    findings.push({
      code: "artifact-report-missing",
      message: "Only bounded, reviewed report artifacts may be retained.",
    });
    return;
  }
  for (const artifact of artifacts) {
    const path = /^\s{10}path:\s*(.+?)\s*$/m.exec(artifact)?.[1];
    if (!ALLOWED_ARTIFACT_PATHS.has(path)) {
      findings.push({
        code: "artifact-path-not-allowlisted",
        message:
          "Artifact paths must contain only the approved summary reports.",
      });
    }
    const retention = /^\s{10}retention-days:\s*(\d+)\s*$/m.exec(artifact)?.[1];
    if (
      !retention ||
      Number(retention) < 1 ||
      Number(retention) > MAX_ARTIFACT_DAYS
    ) {
      findings.push({
        code: "artifact-retention-unbounded",
        message: "Artifact retention must be between one and fourteen days.",
      });
    }
    if (!/^\s{10}if-no-files-found:\s*error\s*$/m.test(artifact)) {
      findings.push({
        code: "artifact-missing-file-not-fatal",
        message: "Missing security evidence must fail the hosted job.",
      });
    }
  }
}

function boundedMinutes(values, upperBound) {
  return (
    values.length > 0 &&
    values.every((value) => value >= 1 && value <= upperBound)
  );
}

function jobBlocks(source) {
  const lines = source.split(/\r?\n/);
  const jobsIndex = lines.findIndex((line) => line === "jobs:");
  if (jobsIndex < 0) return [];
  const blocks = [];
  for (let index = jobsIndex + 1; index < lines.length; index += 1) {
    if (/^  [A-Za-z0-9_-]+:\s*$/.test(lines[index])) {
      const block = [lines[index]];
      for (let cursor = index + 1; cursor < lines.length; cursor += 1) {
        if (
          /^  [A-Za-z0-9_-]+:\s*$/.test(lines[cursor]) ||
          /^\S/.test(lines[cursor])
        ) {
          break;
        }
        block.push(lines[cursor]);
      }
      blocks.push(block.join("\n"));
    } else if (lines[index].trim() && !/^\s/.test(lines[index])) {
      break;
    }
  }
  return blocks;
}

function findingsForTimeouts(source, findings) {
  const timeouts = [
    ...source.matchAll(/^\s*timeout-minutes:\s*(.+?)\s*$/gm),
  ].map((match) => match[1]);
  const jobs = [...source.matchAll(/^\s+job_minutes:\s*(\d+)\s*$/gm)].map(
    (match) => Number(match[1]),
  );
  const suites = [...source.matchAll(/^\s+suite_minutes:\s*(\d+)\s*$/gm)].map(
    (match) => Number(match[1]),
  );
  const jobsWithoutBounds = jobBlocks(source).some((block) => {
    const timeout = /^    timeout-minutes:\s*(.+?)\s*$/m.exec(block)?.[1];
    return !timeout || !timeouts.includes(timeout);
  });
  const valid =
    jobBlocks(source).length > 0 &&
    !jobsWithoutBounds &&
    timeouts.length > 0 &&
    timeouts.every((value) => {
      if (/^\d+$/.test(value))
        return boundedMinutes([Number(value)], MAX_JOB_MINUTES);
      if (value === "${{ matrix.job_minutes }}")
        return boundedMinutes(jobs, MAX_JOB_MINUTES);
      if (value === "${{ matrix.suite_minutes }}")
        return boundedMinutes(suites, MAX_JOB_MINUTES);
      return false;
    });
  if (!valid) {
    findings.push({
      code: "job-timeout-unbounded",
      message:
        "Every job needs a recognized timeout of at most sixty minutes; configured step timeouts must also be bounded.",
    });
  }
  if (
    !boundedMinutes(jobs, MAX_JOB_MINUTES) ||
    !boundedMinutes(suites, MAX_JOB_MINUTES)
  ) {
    findings.push({
      code: "suite-timeout-unbounded",
      message:
        "Boundary matrix timeouts must be explicit and no longer than sixty minutes.",
    });
  }
}

/** Return fixed, non-sensitive policy violations for the hosted CI workflow source. */
export function ciWorkflowPolicyFindings(source) {
  if (typeof source !== "string" || source.length > 128 * 1024) {
    return [
      {
        code: "workflow-source-invalid",
        message: "Workflow source is missing or exceeds its size limit.",
      },
    ];
  }
  const findings = [];
  findingsForTriggers(source, findings);
  findingsForPermissions(source, findings);
  findingsForActions(source, findings);
  findingsForArtifacts(source, findings);
  findingsForTimeouts(source, findings);
  return findings;
}
