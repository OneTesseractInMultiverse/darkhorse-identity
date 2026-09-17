import test from "node:test";
import assert from "node:assert/strict";
import {
  contents,
  parseEnvironment,
  aclUpdates,
  databaseEnvironment,
} from "../../lib/redis-settings.mjs";
const secrets = ["1", "2", "3", "4"].map((value) => value.repeat(64));
test("ACL upgrades preserve credentials and database settings are parsed without executing text", () => {
  const files = contents(secrets);
  const values = parseEnvironment(files[".local/redis.env"]);
  assert.deepEqual(
    aclUpdates(values),
    Object.fromEntries(
      Object.entries(files).filter(([k]) => k.endsWith(".acl")),
    ),
  );
  assert.throws(() =>
    aclUpdates({ ...values, DARKHORSE_REDIS_CACHE_ADMIN_URL: "invalid" }),
  );
  const text =
    "DARKHORSE_DATABASE_PORT=54329\nDARKHORSE_DATABASE_URL=postgres://unit:fixture@localhost/test\nDARKHORSE_DATABASE_INSECURE=true\n";
  assert.equal(databaseEnvironment(text).DARKHORSE_DATABASE_PORT, "54329");
  for (const value of [
    "",
    text + "PATH=x\n",
    text.replace("54329", ""),
    "x".repeat(8193),
  ])
    assert.throws(() => databaseEnvironment(value));
});
test("only the limiter runtime can run bounded counter commands on its exact key", () => {
  const files = contents(secrets);
  const limiter = files[".local/redis-limiter.acl"].split("\n")[1];
  assert.match(limiter, /~darkhorse:limiter:v1/);
  for (const command of [
    "+evalsha",
    "+script|load",
    "+hmget",
    "+hset",
    "+hdel",
    "+time",
  ])
    assert.ok(limiter.includes(command));
  assert.ok(
    !limiter.includes("+del") &&
      !limiter.includes("+flush") &&
      !limiter.includes("~*"),
  );
  assert.ok(
    !files[".local/redis-cache.acl"].split("\n")[1].includes("+evalsha"),
  );
});
test("separate credentials produce restricted ACLs and explicit development endpoints", () => {
  const files = contents(secrets);
  const environment = parseEnvironment(files[".local/redis.env"]);
  assert.equal(environment.DARKHORSE_REDIS_INSECURE, "true");
  assert.notEqual(
    environment.DARKHORSE_REDIS_CACHE_URL,
    environment.DARKHORSE_REDIS_LIMITER_URL,
  );
  for (const role of ["cache", "limiter"]) {
    const acl = files[`.local/redis-${role}.acl`];
    for (const secret of secrets) assert.ok(!acl.includes(secret));
    assert.match(acl, /user default off/);
    assert.match(
      acl,
      new RegExp(
        `user darkhorse-${role} on #[a-f0-9]{64} -@all \\+ping \\+info`,
      ),
    );
    assert.ok(!acl.split("\n")[1].includes("+@all"));
  }
});
test("rejects duplicate/invalid secrets, ports and ambiguous settings files", () => {
  for (const values of [
    [],
    Array(4).fill(secrets[0]),
    [...secrets.slice(0, 3), "invalid"],
  ])
    assert.throws(() => contents(values));
  for (const ports of [[1, 2], [6379, 6379], [65536, 6379], [6379.5, 6380], []])
    assert.throws(() => contents(secrets, ports));
  const text = contents(secrets)[".local/redis.env"];
  for (const value of [
    "",
    text + "PATH=unsafe\n",
    text.replace("DARKHORSE_REDIS_LIMITER_PORT", "DARKHORSE_REDIS_CACHE_PORT"),
    text.replace("=true", "="),
    "a".repeat(16385),
  ])
    assert.throws(() => parseEnvironment(value));
});

test("diagnostic processes receive no operator credentials", async () => {
  const { runtimeEnvironment } = await import("../../lib/redis-settings.mjs");
  const values = parseEnvironment(contents(secrets)[".local/redis.env"]);
  const runtime = runtimeEnvironment({ ...values, PATH: "/fixture/bin" });
  assert.equal(runtime.DARKHORSE_REDIS_CACHE_ADMIN_URL, undefined);
  assert.equal(runtime.DARKHORSE_REDIS_LIMITER_ADMIN_URL, undefined);
  assert.equal(
    runtime.DARKHORSE_REDIS_CACHE_URL,
    values.DARKHORSE_REDIS_CACHE_URL,
  );
  assert.equal(
    runtime.DARKHORSE_REDIS_LIMITER_URL,
    values.DARKHORSE_REDIS_LIMITER_URL,
  );
  assert.equal(runtime.PATH, "/fixture/bin");
  assert.ok(values.DARKHORSE_REDIS_CACHE_ADMIN_URL);
});
