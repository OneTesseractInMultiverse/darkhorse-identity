import assert from "node:assert/strict";
import { test } from "node:test";
import {
  createCleanupEventCollector,
  parseCleanupEvent,
} from "../../lib/benchmark-cleanup-events.mjs";

test("cleanup event parser accepts only fixed aggregate fields", () => {
  assert.deepEqual(
    parseCleanupEvent(
      "maintenance authorization_request_cleanup status=ok batches=10 deleted=1000 backlog_remaining=true oldest_expired_age_ms=1800000 duration_ms=17",
      1234,
    ),
    {
      observedAtUnixMs: 1234,
      status: "ok",
      batches: 10,
      deleted: 1000,
      backlogRemaining: true,
      oldestExpiredAgeMs: 1_800_000,
      durationMs: 17,
    },
  );
  assert.deepEqual(
    parseCleanupEvent(
      "maintenance authorization_request_cleanup status=ok batches=0 deleted=0 backlog_remaining=false oldest_expired_age_ms=none duration_ms=0",
      1235,
    ),
    {
      observedAtUnixMs: 1235,
      status: "ok",
      batches: 0,
      deleted: 0,
      backlogRemaining: false,
      oldestExpiredAgeMs: null,
      durationMs: 0,
    },
  );
  assert.deepEqual(
    parseCleanupEvent(
      "maintenance authorization_request_cleanup status=failed error=unavailable duration_ms=8; retrying next interval.",
      1236,
    ),
    { observedAtUnixMs: 1236, status: "failed", durationMs: 8 },
  );
});

test("cleanup event parser ignores unexpected or identifier-bearing output", () => {
  for (const line of [
    "unrelated server output",
    "maintenance authorization_request_cleanup status=ok batches=999 deleted=999 backlog_remaining=true oldest_expired_age_ms=none duration_ms=2",
    "maintenance authorization_request_cleanup status=ok batches=1 deleted=1 backlog_remaining=false oldest_expired_age_ms=none duration_ms=2 principal=secret",
  ])
    assert.equal(parseCleanupEvent(line, 0), null);
});

test("cleanup event collector handles chunk boundaries and preserves stderr", () => {
  const events = [];
  const forwarded = [];
  const collect = createCleanupEventCollector(
    events,
    (chunk) => forwarded.push(chunk.toString()),
    () => 77,
  );
  collect(
    Buffer.from(
      "maintenance authorization_request_cleanup status=ok batches=1 ",
    ),
  );
  collect(
    Buffer.from(
      "deleted=100 backlog_remaining=false oldest_expired_age_ms=none duration_ms=4\n",
    ),
  );
  collect(Buffer.from("an unrelated error\n"));
  assert.equal(events.length, 1);
  assert.equal(events[0].observedAtUnixMs, 77);
  assert.equal(
    forwarded.join(""),
    "maintenance authorization_request_cleanup status=ok batches=1 deleted=100 backlog_remaining=false oldest_expired_age_ms=none duration_ms=4\nan unrelated error\n",
  );
});
