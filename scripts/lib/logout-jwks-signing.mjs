// Test-only lifecycle coordination. The operator still enforces primary-state
// time/revision fences; uncertain mutation outcomes are never retried here.
import assert from "node:assert/strict";
import { setTimeout as delay } from "node:timers/promises";

export function publicationWait(started, now) {
  assert.ok(Number.isFinite(started) && Number.isFinite(now) && now >= started);
  return Math.max(0, 60100 - (now - started));
}
export async function activatePublishedKey(
  invoke,
  staged,
  {
    now = () => performance.now(),
    sleep = delay,
    publishedAt = now(),
    signal,
  } = {},
) {
  signal?.throwIfAborted();
  assert.match(staged.kid, /^[A-Za-z0-9_-]{43}$/);
  assert.equal(staged.phase, "staged");
  assert.equal(staged.recorded_outcome, "completed");
  assert.ok(Number.isSafeInteger(staged.revision) && staged.revision >= 1);
  const remaining = publicationWait(publishedAt, now());
  if (remaining > 0) await sleep(remaining, undefined, { signal });
  signal?.throwIfAborted();
  const active = JSON.parse(
    (await invoke(["signing-activate", staged.kid, String(staged.revision)]))
      .stdout,
  );
  assert.equal(active.kid, staged.kid);
  assert.equal(active.phase, "active");
  assert.equal(active.revision, staged.revision + 1);
  assert.equal(active.recorded_outcome, "completed");
  return active;
}
