import { test } from "node:test";
import assert from "node:assert/strict";
import {
  objectSettings,
  objectEnvironment,
  objectProject,
} from "../../lib/objects-settings.mjs";
test("local object settings require distinct generated credentials and stable ownership", () => {
  const value = { access: "a".repeat(64), secret: "b".repeat(64) };
  assert.deepEqual(objectSettings(JSON.stringify(value)), value);
  for (const bad of [
    null,
    {},
    { ...value, extra: true },
    { ...value, secret: value.access },
    { ...value, access: "bad" },
  ])
    assert.throws(() => objectSettings(JSON.stringify(bad)));
  assert.throws(() => objectSettings("x".repeat(1025)));
  assert.match(objectProject("/work/one"), /^darkhorse-objects-[a-f0-9]{10}$/);
  assert.notEqual(objectProject("/work/one"), objectProject("/work/two"));
  const env = objectEnvironment(value);
  assert.equal(env.DARKHORSE_OBJECTS_LOCAL_HTTP, "true");
  assert.equal(env.DARKHORSE_OBJECTS_ENDPOINT, "http://127.0.0.1:9009");
  assert.equal(env.DARKHORSE_OBJECTS_ACCESS_KEY, value.access);
});
