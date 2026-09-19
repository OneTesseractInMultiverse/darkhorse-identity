import assert from "node:assert/strict";
import { test } from "node:test";
import { profileChannel } from "../../lib/benchmark-profile-channel.mjs";
import { STAGES, UPPER_US } from "../../lib/benchmark-profile-model.mjs";
const frame =
  "DARKHORSE_PROFILE " +
  JSON.stringify({
    schema: 1,
    upper_us: UPPER_US,
    stages: Object.fromEntries(
      STAGES.map((name) => [
        name,
        {
          ok: 0,
          error: 0,
          cancelled: 0,
          sum_us: 0,
          max_us: 0,
          buckets: Array(22).fill(0),
        },
      ]),
    ),
  }) +
  "\n";
function fake() {
  let timeout,
    cleared = 0;
  const channel = profileChannel({
    schedule: (fn) => {
      timeout = fn;
      return 1;
    },
    cancel: (id) => {
      assert.equal(id, 1);
      cleared++;
    },
  });
  return { channel, expire: () => timeout(), cleared: () => cleared };
}
test("owned process requests collect fragmented frames and reject overlapping requests", async () => {
  const { channel, cleared } = fake();
  channel.accept("startup log\n");
  const result = channel.request(() => true);
  await assert.rejects(
    channel.request(() => true),
    /pending/,
  );
  channel.accept(frame.slice(0, 40));
  channel.accept(frame.slice(40));
  assert.equal((await result).stages.total.count, 0);
  assert.equal(cleared(), 1);
  const again = channel.request(() => {
    channel.accept(frame);
    return true;
  });
  assert.equal((await again).schema, 1);
});
test("missing, malformed and unsolicited frames make measurements fail closed", async () => {
  for (const fail of [
    (c) => c.accept("DARKHORSE_PROFILE SENSITIVE\n"),
    (c) => c.accept("x".repeat(16385)),
    (c) => c.accept("x".repeat(16385) + "\n"),
    (c) => c.close(),
  ]) {
    const { channel } = fake();
    const result = channel.request(() => true);
    fail(channel);
    await assert.rejects(
      result,
      (error) => !error.message.includes("SENSITIVE"),
    );
    channel.accept("ignored after failure\n");
    channel.close();
    await assert.rejects(channel.request(() => true));
  }
  const { channel, expire } = fake();
  const result = channel.request(() => true);
  expire();
  await assert.rejects(result, /timed out/);
  const unsolicited = fake().channel;
  unsolicited.accept(frame);
  await assert.rejects(
    unsolicited.request(() => true),
    /unsolicited/,
  );
  for (const signal of [
    () => false,
    () => {
      throw new Error("SENSITIVE");
    },
  ])
    await assert.rejects(
      fake().channel.request(signal),
      /^Error: Cannot signal benchmark process\.$/,
    );
});
