import { operatorSummary } from "./benchmark-operator-model.mjs";

async function lane(worker, { durationMs, clock, sleep, invoke }, start) {
  const commands = [];
  for (let index = 0; index < 4; index++) {
    const scheduledMs = start + (index * durationMs) / 4;
    while (clock() < scheduledMs) await sleep(scheduledMs - clock());
    const operation = index % 2 === 0 ? "account.list" : "application.list";
    const startMs = clock();
    const result = await invoke(worker, operation);
    commands.push({
      worker,
      operation,
      scheduledMs,
      startMs,
      endMs: clock(),
      ...result,
    });
  }
  return commands;
}
export async function measureOperatorPhase({ load, ...ports }) {
  const start = ports.clock();
  // Drain both native lanes and HTTP before the fixture can be torn down.
  const results = await Promise.allSettled([
    load(),
    lane(0, ports, start),
    lane(1, ports, start),
  ]);
  const failure = results.find((result) => result.status === "rejected");
  if (failure) throw failure.reason;
  const measured = results[0].value;
  return {
    ...measured,
    summary: {
      ...measured.summary,
      operators: operatorSummary(
        results.slice(1).flatMap((result) => result.value),
        measured.rows,
      ),
    },
  };
}
