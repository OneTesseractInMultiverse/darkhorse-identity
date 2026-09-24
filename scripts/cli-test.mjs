import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { resolve } from "node:path";

process.chdir(resolve(import.meta.dirname, ".."));
const executable = resolve(
  process.env.CARGO_TARGET_DIR ?? "target",
  "debug/darkhorse-server",
);
const environment = {
  ...(process.env.LLVM_PROFILE_FILE
    ? { LLVM_PROFILE_FILE: process.env.LLVM_PROFILE_FILE }
    : {}),
  PATH: process.env.PATH,
  NO_COLOR: "1",
  DARKHORSE_DATABASE_URL: "invalid-secret-database-setting",
  DARKHORSE_HTTP_HOST: "invalid-host-setting",
};
function invoke(args, input = "") {
  return new Promise((resolveResult, reject) => {
    const child = execFile(
      executable,
      args,
      {
        env: environment,
        timeout: 10000,
        killSignal: "SIGKILL",
        maxBuffer: 128 * 1024,
      },
      (error, stdout, stderr) => {
        if (error && !Number.isInteger(error.code))
          return reject(
            new Error(
              "CLI test did not exit within its bounded execution contract.",
            ),
          );
        resolveResult({ code: error?.code ?? 0, stdout, stderr });
      },
    );
    child.stdin.on("error", () => {});
    child.stdin.end(input);
  });
}
async function information() {
  for (const args of [
    ["--help"],
    ["--version"],
    ["help", "operator"],
    ["operator", "--help"],
    ["operator", "account", "--help"],
    ["operator", "signing", "import", "--help"],
    ["operator", "signing", "activate", "--help"],
    ["operator", "signing", "retire", "--help"],
  ]) {
    const result = await invoke(args);
    assert.equal(result.code, 0);
    assert.ok(result.stdout.length > 0 && result.stdout.length < 16384);
    assert.equal(result.stderr, "");
    assert.ok(!result.stdout.includes("\x1b"));
  }
}
async function failures() {
  const marker = "test-secret-do-not-echo";
  const key = "-" + "A".repeat(42);
  for (const action of ["activate", "retire"]) {
    for (const command of [
      ["operator", "signing", action],
      [`signing-${action}`],
    ]) {
      const result = await invoke([
        "--output",
        "json",
        "--yes",
        ...command,
        key,
        "1",
      ]);
      assert.equal(result.code, 1);
      assert.equal(JSON.parse(result.stderr).error.code, "operation_failed");
      assert.equal(result.stdout, "");
    }
  }

  for (const args of [
    ["bootstrap", "--password", marker],
    ["account", marker],
    ["operator", "unknown", marker],
    ["operator", "signing", "activate", marker, "0"],
    ["serve", marker],
    ["migrate;echo", marker],
    ["x".repeat(1025)],
    Array(33).fill(marker),
  ]) {
    const result = await invoke(args);
    assert.equal(result.code, 2);
    assert.equal(result.stdout, "");
    assert.ok(!result.stderr.includes(marker));
    assert.ok(!result.stderr.includes("invalid-secret-database-setting"));
  }
  for (const args of [
    ["migrate"],
    ["operator", "migrate"],
    ["bootstrap", "--stdin"],
    ["signing-import", "--stdin", "0"],
    ["limiter-fence"],
    ["signing-status"],
  ]) {
    const result = await invoke(args, marker);
    assert.equal(result.code, 3);
    assert.equal(result.stdout, "");
    assert.match(result.stderr, /not confirmed/);
    assert.ok(!result.stderr.includes(marker));
  }
  for (const input of [
    "",
    "{}",
    JSON.stringify({
      email: "a@b.com",
      first_name: "A",
      last_name: "B",
      password: marker,
      unexpected: true,
    }),
    "[".repeat(200) + "]".repeat(200),
    "x".repeat(16385),
  ]) {
    const result = await invoke(
      ["operator", "bootstrap", "--stdin", "--yes"],
      input,
    );
    assert.equal(result.code, 1);
    assert.equal(result.stdout, "");
    assert.match(result.stderr, /bootstrap input/i);
    assert.ok(!result.stderr.includes(marker));
  }
  const interactive = await invoke(["bootstrap", "--yes"]);
  assert.equal(interactive.code, 1);
  assert.match(interactive.stderr, /requires a terminal/);
  assert.equal(interactive.stdout, "");
  const confirmation = await invoke([
    "--output",
    "json",
    "operator",
    "migrate",
  ]);
  assert.equal(confirmation.code, 3);
  assert.equal(confirmation.stdout, "");
  assert.equal(
    JSON.parse(confirmation.stderr).error.code,
    "confirmation_required",
  );
  const json = await invoke([
    "--output",
    "json",
    "operator",
    "migrate",
    "--yes",
  ]);
  assert.equal(json.code, 1);
  assert.equal(json.stdout, "");
  const value = JSON.parse(json.stderr);
  assert.equal(value.schema_version, 1);
  assert.equal(value.ok, false);
  assert.equal(value.error.code, "operation_failed");
  assert.ok(!json.stderr.includes(environment.DARKHORSE_DATABASE_URL));
}
await information();
await failures();
console.log(
  "CLI subprocess checks passed: service-free help/version, redacted parsing, bounded stdin, confirmations, noninteractive refusal and JSON failures.",
);
