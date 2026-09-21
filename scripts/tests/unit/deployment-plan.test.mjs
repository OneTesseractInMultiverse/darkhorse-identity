import test from "node:test";
import assert from "node:assert/strict";
import {
  settings,
  secretFiles,
  environment,
  operatorArgs,
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
  const secrets = Array.from({ length: 7 }, (_, i) =>
    (i + 1).toString(16).repeat(64),
  );
  const files = secretFiles(secrets);
  assert.match(files["runtime-db"], /^postgres:\/\/darkhorse_runtime:/);
  assert.match(files["owner-db"], /^postgres:\/\/darkhorse_owner:/);
  assert.match(files["limiter-url"], /^rediss:\/\/darkhorse-limiter:/);
  assert.match(files["limiter-admin-url"], /^rediss:\/\/operator:/);
  assert.ok(!files["cache-acl"].includes(secrets[3]));
  assert.ok(files["cache-acl"].includes("user default off"));
  assert.throws(() => secretFiles(Array(7).fill(secrets[0])));
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
