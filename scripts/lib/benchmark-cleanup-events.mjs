export function parseCleanupEvent(line, observedAtUnixMs) {
  const success =
    /^maintenance authorization_request_cleanup status=ok batches=(\d+) deleted=(\d+) backlog_remaining=(true|false) oldest_expired_age_ms=(none|\d+) duration_ms=(\d+)$/.exec(
      line,
    );
  if (success) {
    const [, rawBatches, rawDeleted, backlog, rawAge, rawDuration] = success;
    const batches = Number(rawBatches);
    const deleted = Number(rawDeleted);
    const oldestExpiredAgeMs = rawAge === "none" ? null : Number(rawAge);
    const durationMs = Number(rawDuration);
    if (
      batches > 10 ||
      deleted > 1000 ||
      deleted > batches * 100 ||
      ![
        batches,
        deleted,
        durationMs,
        ...(oldestExpiredAgeMs === null ? [] : [oldestExpiredAgeMs]),
      ].every(Number.isSafeInteger)
    )
      return null;
    return {
      observedAtUnixMs,
      status: "ok",
      batches,
      deleted,
      backlogRemaining: backlog === "true",
      oldestExpiredAgeMs,
      durationMs,
    };
  }

  const failure =
    /^maintenance authorization_request_cleanup status=failed error=unavailable duration_ms=(\d+); retrying next interval\.$/.exec(
      line,
    );
  if (!failure) return null;
  const durationMs = Number(failure[1]);
  if (!Number.isSafeInteger(durationMs)) return null;
  return { observedAtUnixMs, status: "failed", durationMs };
}

export function createCleanupEventCollector(
  events,
  forward = (chunk) => process.stderr.write(chunk),
  now = () => performance.timeOrigin + performance.now(),
) {
  let pending = "";
  const accept = (chunk) => {
    forward(chunk);
    pending += typeof chunk === "string" ? chunk : chunk.toString("utf8");
    const lines = pending.split("\n");
    pending = lines.pop();
    for (const rawLine of lines) collect(rawLine.replace(/\r$/, ""));
  };
  const flush = () => {
    if (pending) collect(pending.replace(/\r$/, ""));
    pending = "";
  };
  const collect = (line) => {
    const event = parseCleanupEvent(line, now());
    if (event) events.push(event);
  };
  accept.flush = flush;
  return accept;
}
