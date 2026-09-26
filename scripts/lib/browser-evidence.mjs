export const browserPhases = Object.freeze([
  "setup",
  "localization",
  "language-preferences",
  "sign-in",
  "rotation",
  "registration",
  "provider",
  "directory",
  "catalog",
  "personal-keys",
  "profiles",
  "console-language",
  "email",
  "invitations",
  "sessions",
  "logout-revocation",
  "page-security",
]);
export const browserCompleted = "DARKHORSE_BROWSER_COMPLETED:1";
const expected = [
  ...browserPhases.flatMap((phase) => [
    `DARKHORSE_BROWSER:${phase}:started`,
    `DARKHORSE_BROWSER:${phase}:passed`,
  ]),
  browserCompleted,
];
export function browserEvidence(stdout) {
  const markers = stdout
    .split("\n")
    .filter((line) => line.startsWith("DARKHORSE_BROWSER"));
  const diagnosticLines = stdout
    .split("\n")
    .filter((line) => line.startsWith("DARKHORSE_DIAGNOSTIC:"));
  const diagnostic =
    diagnosticLines.length === 1
      ? decodeDiagnostic(
          diagnosticLines[0].slice("DARKHORSE_DIAGNOSTIC:".length),
        )
      : undefined;
  const completed =
    diagnosticLines.length === 0 &&
    markers.length === expected.length &&
    markers.every((line, i) => line === expected[i]);
  let prefix = 0;
  while (prefix < expected.length && markers[prefix] === expected[prefix])
    prefix++;
  return {
    ...(diagnostic ? { diagnostic } : {}),
    status: completed ? "completed" : "incomplete",
    phase:
      prefix === 0
        ? "not started"
        : prefix >= expected.length - 1
          ? "cleanup"
          : browserPhases[Math.floor((prefix - 1) / 2)],
  };
}
export function browserMarker(phase, state) {
  if (!browserPhases.includes(phase) || !["started", "passed"].includes(state))
    throw new Error("Unknown browser test phase.");
  return `DARKHORSE_BROWSER:${phase}:${state}`;
}
export async function browserPhase(phase, action) {
  console.log(browserMarker(phase, "started"));
  try {
    const result = await action();
    console.log(browserMarker(phase, "passed"));
    return result;
  } catch (error) {
    console.log(
      `DARKHORSE_DIAGNOSTIC:${JSON.stringify(browserFailure(error, process.cwd()))}`,
    );
    throw error;
  }
}

const diagnosticTypes = [
  "Error",
  "AssertionError",
  "TimeoutError",
  "TypeError",
  "SyntaxError",
];
function location(file, line, column) {
  if (
    !/^scripts\/(?:lib\/)?[a-z][a-z0-9-]{0,80}\.mjs$/.test(file) ||
    ![line, column].every((n) => Number.isInteger(n) && n > 0 && n <= 100000)
  )
    return;
  return { file, line, column };
}
export function browserFailure(error, root) {
  const type = diagnosticTypes.includes(error?.name) ? error.name : "Error";
  const locations = [];
  const stack =
    typeof error?.stack === "string" ? error.stack.slice(0, 8192) : "";
  for (const line of stack.split("\n").slice(1, 41)) {
    const match = /^\s+at [^(]*\((?:file:\/\/)?(.+):(\d+):(\d+)\)$/.exec(line);
    if (!match || !match[1].startsWith(root + "/")) continue;
    const found = location(
      match[1].slice(root.length + 1),
      Number(match[2]),
      Number(match[3]),
    );
    if (found) locations.push(found);
    if (locations.length === 4) break;
  }
  return { type, locations };
}
function decodeDiagnostic(value) {
  if (value.length > 1024) return;
  try {
    const parsed = JSON.parse(value);
    if (
      !diagnosticTypes.includes(parsed.type) ||
      !Array.isArray(parsed.locations) ||
      parsed.locations.length > 4
    )
      return;
    const locations = parsed.locations.map((entry) =>
      location(entry.file, entry.line, entry.column),
    );
    if (locations.some((entry) => !entry)) return;
    return { type: parsed.type, locations };
  } catch {
    return;
  }
}
