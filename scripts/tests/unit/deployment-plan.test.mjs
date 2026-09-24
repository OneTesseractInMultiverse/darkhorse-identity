import test from "node:test";
import assert from "node:assert/strict";
import {
  settings,
  secretFiles,
  environment,
  operatorArgs,
  operatorWorkload,
} from "../../lib/deployment-plan.mjs";
const image = "sha256:" + "a".repeat(64);
const input = {
  name: "test",
  origin: "https://identity.localhost:9443",
  image,
  edgeImage: image,
};
test("canonical deployment identity rejects ambiguous names, issuer routes and mutable images", () => {
  const value = settings(input);
  assert.equal(value.project, "darkhorse-stack-test");
  assert.equal(value.port, 9443);
  assert.equal(value.host, "identity.localhost");
  for (const changed of [
    { name: "../prod" },
    { name: "x y" },
    { name: "MAIN" },
    { origin: "http://identity.localhost:9443" },
    { origin: "https://u:p@identity.test" },
    { origin: "https://identity.test/path" },
    { origin: "https://identity.test/?q=x" },
    { origin: "https://identity.test/#x" },
    { origin: "https://api" },
    { origin: "https://127.0.0.1:9443" },
    { image: "darkhorse:latest" },
    { edgeImage: "caddy:latest" },
    { origin: "https://identity.localhost:42" },
    { origin: "https://identity.localhost:9443/" },
  ])
    assert.throws(() => settings({ ...input, ...changed }));
  const env = environment(value, "/private/stack");
  assert.equal(env.DARKHORSE_STACK_DIR, "/private/stack");
  assert.equal(env.DARKHORSE_STACK_ORIGIN, input.origin);
  assert.ok(!JSON.stringify(env).includes("password"));
});
test("dedicated file credentials preserve role and TLS separation", () => {
  const secrets = Array.from({ length: 8 }, (_, i) =>
    (i + 1).toString(16).repeat(64),
  );
  const files = secretFiles(secrets);
  assert.match(files["runtime-db"], /^postgres:\/\/darkhorse_runtime:/);
  assert.match(files["operator-db"], /^postgres:\/\/darkhorse_operator:/);
  assert.notEqual(files["operator-password"], files["owner-password"]);
  assert.notEqual(files["operator-password"], files["runtime-password"]);
  assert.match(files["owner-db"], /^postgres:\/\/darkhorse_owner:/);
  assert.match(files["limiter-url"], /^rediss:\/\/darkhorse-limiter:/);
  assert.match(files["limiter-admin-url"], /^rediss:\/\/operator:/);
  assert.ok(!files["cache-acl"].includes(secrets[3]));
  assert.ok(files["cache-acl"].includes("user default off"));
  assert.throws(() => secretFiles(Array(8).fill(secrets[0])));
  assert.throws(() => secretFiles(["bad"]));
});
test("one-shot commands use explicit allowlists and never accept credential arguments", () => {
  assert.deepEqual(operatorArgs("bootstrap", []), ["bootstrap"]);
  assert.deepEqual(operatorArgs("bootstrap", ["--stdin"]), [
    "bootstrap",
    "--stdin",
  ]);
  assert.deepEqual(operatorArgs("signing-generate", ["0"]), [
    "signing-generate",
    "0",
  ]);
  assert.deepEqual(operatorArgs("signing-retire", ["a".repeat(43), "2"]), [
    "signing-retire",
    "a".repeat(43),
    "2",
  ]);
  for (const [name, args] of [
    ["serve", []],
    ["migrate", ["password"]],
    ["bootstrap", ["--password", "secret"]],
    ["signing-generate", ["-1"]],
    ["signing-generate", ["9223372036854775808"]],
    ["signing-activate", ["invalid", "1"]],
    ["sql", []],
    ["limiter-activate", ["--force"]],
  ])
    assert.throws(() => operatorArgs(name, args));
});

test("only reviewed migrations select the owner workload", () => {
  assert.equal(operatorWorkload("migrate", []), "migrator");
  for (const command of [
    "limiter-fence",
    "limiter-activate",
    "signing-status",
    "redis-status",
    "bootstrap",
  ])
    assert.equal(operatorWorkload(command, []), "operator");
  assert.throws(() => operatorWorkload("migrate", ["--extra"]));
  assert.throws(() => operatorWorkload("serve", []));
});

test("activation inspection accepts one nonzero UUID and uses nonowner authority", () => {
  const id = "00000000-0000-0000-0000-000000000123";
  assert.deepEqual(operatorArgs("limiter-inspect", [id]), [
    "operator",
    "limiter",
    "inspect",
    id,
  ]);
  assert.equal(operatorWorkload("limiter-inspect", [id]), "operator");
  for (const args of [
    [],
    [id, id],
    ["--force"],
    ["00000000-0000-0000-0000-000000000000"],
    [id + "\n"],
    ["$(touch unexpected)"],
  ])
    assert.throws(() => operatorArgs("limiter-inspect", args));
});

test("signing inspection accepts one nonzero UUID and uses nonowner authority", () => {
  const id = "00000000-0000-0000-0000-000000000123";
  assert.deepEqual(operatorArgs("signing-inspect", [id]), [
    "operator",
    "signing",
    "inspect",
    id,
  ]);
  assert.equal(operatorWorkload("signing-inspect", [id]), "operator");
  for (const args of [
    [],
    [id, id],
    ["--force"],
    ["00000000-0000-0000-0000-000000000000"],
    [id + "\n"],
    ["$(touch unexpected)"],
  ])
    assert.throws(() => operatorArgs("signing-inspect", args));
});

test("migration inspection uses only the dedicated owner workload", () => {
  const id = "00000000-0000-0000-0000-000000000123";
  assert.deepEqual(operatorArgs("migration-inspect", [id]), [
    "operator",
    "migrate",
    "inspect",
    id,
  ]);
  assert.equal(operatorWorkload("migration-inspect", [id]), "migrator");
  for (const args of [
    [],
    [id, id],
    ["--force"],
    ["00000000-0000-0000-0000-000000000000"],
    [id + "\n"],
  ])
    assert.throws(() => operatorArgs("migration-inspect", args));
});
