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
  const completed =
    markers.length === expected.length &&
    markers.every((line, i) => line === expected[i]);
  let prefix = 0;
  while (prefix < expected.length && markers[prefix] === expected[prefix])
    prefix++;
  return {
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
  const result = await action();
  console.log(browserMarker(phase, "passed"));
  return result;
}
