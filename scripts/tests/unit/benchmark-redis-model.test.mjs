import assert from "node:assert/strict";
import { test } from "node:test";
import { redisMetrics } from "../../lib/benchmark-redis-model.mjs";
const valid =
  "# Memory\r\nused_memory:2048\r\nused_memory_peak:4096\r\ntotal_commands_processed:12\r\nused_cpu_user:0.25\r\nused_cpu_sys:1.2\r\nprivate_context:never-publish-this\r\n";
test("Redis measurements retain only fixed numeric cost fields", () => {
  assert.deepEqual(redisMetrics(valid), {
    usedBytes: 2048,
    peakUsedBytes: 4096,
    commands: 12,
    userCpuSeconds: 0.25,
    systemCpuSeconds: 1.2,
  });
});
test("missing, duplicate, unsafe and malformed measurements fail without echoing input", () => {
  for (const text of [
    null,
    "",
    valid.repeat(2),
    "x".repeat(65537),
    valid.replace("2048", "-1"),
    valid.replace("4096", "100"),
    valid.replace("2048", "9007199254740992"),
    valid.replace("1.2", "NaN"),
    valid.replace("1.2", "Infinity"),
    valid.replace("12\r", "1.5\r"),
    valid.replace("2048", "02048"),
    valid.replace("used_memory:", "absent:"),
  ]) {
    assert.throws(() => redisMetrics(text), {
      message: "Invalid Redis benchmark metrics.",
    });
  }
});
