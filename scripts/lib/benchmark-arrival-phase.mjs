import { arrivalPlan, arrivalSummary } from "./benchmark-arrival-model.mjs";
import { runArrivals } from "./benchmark-arrival-load.mjs";

async function mutate(change, plan, clock, sleep) {
  await sleep(plan.durationMs / 2);
  const startMs = clock();
  await change();
  return { startMs, acknowledgedMs: clock() };
}
export async function measureArrivals({ name, settings, change, ...ports }) {
  const plan = arrivalPlan(settings);
  const load = runArrivals({ settings, ...ports });
  const mutation = change
    ? mutate(change, plan, ports.clock, ports.sleep)
    : Promise.resolve(null);
  const [loaded, changed] = await Promise.allSettled([load, mutation]);
  if (loaded.status === "rejected") throw loaded.reason;
  if (changed.status === "rejected") throw changed.reason;
  return {
    rows: loaded.value.rows,
    summary: {
      ...arrivalSummary(name, plan, loaded.value),
      change: changed.value,
    },
  };
}
