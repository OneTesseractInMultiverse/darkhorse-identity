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
function invoke(args, input = "", extraEnvironment = {}) {
  return new Promise((resolveResult, reject) => {
    const child = execFile(
      executable,
      args,
      {
        env: { ...environment, ...extraEnvironment },
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
    ["operator", "migrate", "--help"],
    ["operator", "migrate", "inspect", "--help"],
    ["operator", "account", "--help"],
    ["operator", "account", "list", "--help"],
    ["operator", "application", "list", "--help"],
    ["operator", "client", "list", "--help"],
    ["operator", "client", "update", "--help"],
    ["operator", "client", "secret", "list", "--help"],
    ["operator", "client", "secret", "retire", "--help"],
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
  const inspection = await invoke([
    "--output",
    "json",
    "operator",
    "migrate",
    "inspect",
    "00000000-0000-0000-0000-000000000123",
  ]);
  assert.equal(inspection.code, 1);
  assert.equal(JSON.parse(inspection.stderr).error.code, "operation_failed");
  assert.equal(inspection.stdout, "");
  assert.ok(!inspection.stderr.includes(environment.DARKHORSE_DATABASE_URL));

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
    ["operator", "account", "list", "--limit", "26"],
    ["operator", "application", "list", "--limit", "0"],
    ["operator", "client", "list", "invalid"],
    ["--output", "json", "operator", "account", "list"],
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
  const secretIds = [
    "00000000-0000-0000-0000-000000000001",
    "00000000-0000-0000-0000-000000000002",
  ];
  for (const operation of ["list", "retire"]) {
    const args = [
      "--auth-stdin",
      "--output",
      "json",
      "operator",
      "client",
      "secret",
      operation,
      ...secretIds,
      ...(operation === "retire"
        ? ["00000000-0000-0000-0000-000000000003", "0"]
        : []),
    ];
    const auth = { email: "a@b.com", password: marker };
    const badReason = await invoke(
      [...args, "--yes"],
      JSON.stringify({
        ...auth,
        ...(operation === "list" ? { reason: "not accepted" } : {}),
      }),
    );
    assert.equal(badReason.code, 2);
    const valid = {
      ...auth,
      ...(operation === "retire" ? { reason: "Fixture retirement" } : {}),
    };
    if (operation === "retire")
      assert.equal((await invoke(args, JSON.stringify(valid))).code, 3);
    const unavailable = await invoke([...args, "--yes"], JSON.stringify(valid));
    assert.equal(unavailable.code, 1);
    assert.equal(unavailable.stdout, "");
    assert.ok(!unavailable.stderr.includes(marker));
    assert.ok(!unavailable.stderr.includes(environment.DARKHORSE_DATABASE_URL));
    const oversized = await invoke([...args, "--yes"], "x".repeat(16385));
    assert.equal(oversized.code, 1);
    assert.equal(oversized.stdout, "");
  }
  const listing = await invoke(
    ["--auth-stdin", "--output", "json", "operator", "account", "list"],
    JSON.stringify({ email: "a@b.com", password: marker }),
  );
  assert.equal(listing.code, 1);
  assert.equal(listing.stdout, "");
  assert.equal(JSON.parse(listing.stderr).error.code, "operation_failed");
  assert.ok(!listing.stderr.includes(marker));
  assert.ok(!listing.stderr.includes(environment.DARKHORSE_DATABASE_URL));
  for (const args of [
    ["operator", "application", "list", "--search=--literal"],
    ["operator", "client", "list", "00000000-0000-0000-0000-000000000001"],
    ["operator", "application", "show", "00000000-0000-0000-0000-000000000001"],
    [
      "operator",
      "client",
      "show",
      "00000000-0000-0000-0000-000000000001",
      "00000000-0000-0000-0000-000000000002",
    ],
  ]) {
    const result = await invoke(
      ["--auth-stdin", "--output", "json", ...args],
      JSON.stringify({ email: "a@b.com", password: marker }),
    );
    assert.equal(result.code, 1);
    assert.equal(result.stdout, "");
    assert.equal(JSON.parse(result.stderr).error.code, "operation_failed");
    assert.ok(!result.stderr.includes(marker));
    assert.ok(!result.stderr.includes(environment.DARKHORSE_DATABASE_URL));
  }
  for (const args of [
    ["application", "list"],
    ["application", "show", "00000000-0000-0000-0000-000000000001"],
    [
      "client",
      "show",
      "00000000-0000-0000-0000-000000000001",
      "00000000-0000-0000-0000-000000000002",
    ],
  ]) {
    const reasonOnRead = await invoke(
      ["--auth-stdin", "--output", "json", "operator", ...args],
      JSON.stringify({
        email: "a@b.com",
        password: marker,
        reason: "unsupported",
      }),
    );
    assert.equal(reasonOnRead.code, 2);
    assert.equal(reasonOnRead.stdout, "");
    assert.ok(!reasonOnRead.stderr.includes(marker));
  }
  for (const operation of ["create", "update"]) {
    const args = [
      "--auth-stdin",
      "--output",
      "json",
      "operator",
      "application",
      operation,
      ...(operation === "update"
        ? ["00000000-0000-0000-0000-000000000001", "0"]
        : []),
      "--name",
      "Fixture",
      "--owner",
      "00000000-0000-0000-0000-000000000001",
      "--status",
      "active",
    ];
    const unconfirmed = await invoke(
      args,
      JSON.stringify({
        email: "a@b.com",
        password: marker,
        reason: "Approved fixture",
      }),
    );
    assert.equal(unconfirmed.code, 3);
    assert.equal(unconfirmed.stdout, "");
    assert.ok(!unconfirmed.stderr.includes(marker));
    const missingReason = await invoke(
      [...args, "--yes"],
      JSON.stringify({ email: "a@b.com", password: marker }),
    );
    assert.equal(missingReason.code, 2);
    assert.equal(missingReason.stdout, "");
    assert.ok(!missingReason.stderr.includes(marker));
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
await languages();
await information();
await failures();
await clientInput();
console.log(
  "CLI subprocess checks passed: service-free help/version, redacted parsing, bounded stdin, confirmations, noninteractive refusal and JSON failures.",
);

async function clientInput() {
  const command = [
    "operator",
    "client",
    "update",
    "00000000-0000-0000-0000-000000000001",
    "00000000-0000-0000-0000-000000000002",
    "0",
  ];
  const marker = "source-only-secret-do-not-echo";
  const input = {
    authentication: {
      email: "a@b.com",
      password: marker,
      reason: "Approved update",
    },
    client: {
      name: "Client",
      active: true,
      refresh_tokens: false,
      redirect_uris: ["https://client.example/callback"],
      resource_ids: [],
      scope_ids: [],
      token_endpoint_auth_method: "client_secret_basic",
    },
  };
  for (const [args, code] of [
    [["--yes", ...command], 2],
    [["--auth-stdin", ...command], 3],
  ]) {
    const result = await invoke(args, JSON.stringify(input));
    assert.equal(result.code, code);
    assert.equal(result.stdout, "");
    assert.ok(!result.stderr.includes(marker));
  }
  const missingRefresh = structuredClone(input);
  delete missingRefresh.client.refresh_tokens;
  const missingReason = structuredClone(input);
  delete missingReason.authentication.reason;
  for (const data of [
    "{}",
    "x".repeat(32769),
    JSON.stringify(missingRefresh),
    JSON.stringify(missingReason),
    JSON.stringify({ ...input, unexpected: marker }),
    JSON.stringify({ ...input, client: { ...input.client, secret: marker } }),
  ]) {
    const result = await invoke(
      ["--yes", "--auth-stdin", "--output", "json", ...command],
      data,
    );
    assert.equal(result.code, 2);
    assert.equal(result.stdout, "");
    assert.ok(!result.stderr.includes(marker));
    assert.ok(!result.stderr.includes(environment.DARKHORSE_DATABASE_URL));
  }
  const valid = await invoke(
    ["--yes", "--auth-stdin", "--output", "json", ...command],
    JSON.stringify(input),
  );
  assert.equal(valid.code, 1);
  assert.equal(valid.stdout, "");
  assert.ok(!valid.stderr.includes(marker));
  assert.ok(!valid.stderr.includes(environment.DARKHORSE_DATABASE_URL));
}

async function languages() {
  for (const args of [
    ["--locale", "es", "--help"],
    ["operator", "account", "show", "--help", "--locale=es"],
    ["--locale", "es", "help", "operator"],
  ]) {
    const help = await invoke(args);
    assert.equal(help.code, 0);
    assert.match(help.stdout, /Uso:/);
    assert.doesNotMatch(
      help.stdout,
      /Usage:|Confirm a mutation|Secret arguments/,
    );
  }
  const cases = [
    ["operator", "bootstrap"],
    ["operator", "account"],
    ["operator", "migrate"],
    ["operator", "migrate", "--yes"],
    [
      "--auth-stdin",
      "operator",
      "account",
      "show",
      "00000000-0000-0000-0000-000000000001",
    ],
  ];
  for (const args of cases) {
    const en = await invoke(
      ["--locale", "en", "--output", "json", ...args],
      "{}",
    );
    const es = await invoke(
      ["--locale", "es", "--output", "json", ...args],
      "{}",
    );
    assert.deepEqual(
      es,
      en,
      "JSON diagnostics, streams and exit statuses remain byte-identical",
    );
    assert.ok(!es.stderr.includes(environment.DARKHORSE_DATABASE_URL));
  }
  const failure = await invoke([
    "--locale",
    "es",
    "operator",
    "migrate",
    "--yes",
  ]);
  assert.equal(failure.code, 1);
  assert.match(failure.stderr, /Configuración de base de datos no válida/);
  const ambient = await invoke(["--help"], "", { LANG: "es_CR.UTF-8" });
  assert.match(ambient.stdout, /Uso:/);
  const explicit = await invoke(["--locale", "en", "--help"], "", {
    DARKHORSE_CLI_LOCALE: "invalid-private-value",
  });
  assert.equal(explicit.code, 0);
  assert.match(explicit.stdout, /Usage:/);
  const invalid = await invoke(["--help"], "", {
    DARKHORSE_CLI_LOCALE: "invalid-private-value",
  });
  assert.equal(invalid.code, 1);
  assert.doesNotMatch(invalid.stderr, /invalid-private-value/);
}
