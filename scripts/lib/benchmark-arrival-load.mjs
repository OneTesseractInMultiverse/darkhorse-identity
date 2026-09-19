import { attempt } from "./benchmark-load.mjs";
import {
  arrivalPlan,
  arrivalAction,
  arrivalDrop,
  arrivalResult,
} from "./benchmark-arrival-model.mjs";

async function waitUntil(targetMs, clock, sleep) {
  for (let now = clock(); now < targetMs; now = clock())
    await sleep(targetMs - now);
}
function dispatch(state, index, selected, scheduledMs, perform, clock) {
  const job = attempt(index, selected, perform, clock)
    .then((row) => {
      state.rows[index] = arrivalResult(row, scheduledMs);
    })
    .catch((error) => {
      state.errors.push(error);
    });
  state.pending.add(job);
  state.peakInFlight = Math.max(state.peakInFlight, state.pending.size);
  void job.then(() => state.pending.delete(job));
}
async function schedule(plan, state, ports) {
  const { clock, sleep, select, perform } = ports;
  for (let index = 0; index < plan.count; index++) {
    const scheduledMs = state.startMs + index * plan.intervalMs;
    await waitUntil(scheduledMs, clock, sleep);
    const selected = select(index);
    const nowMs = clock();
    const action = arrivalAction(plan, {
      scheduledMs,
      nowMs,
      endMs: state.startMs + plan.durationMs,
      inFlight: state.pending.size,
    });
    if (action === "dispatch")
      dispatch(state, index, selected, scheduledMs, perform, clock);
    else
      state.rows[index] = arrivalDrop(
        index,
        selected,
        action,
        scheduledMs,
        nowMs,
      );
  }
  await waitUntil(state.startMs + plan.durationMs, clock, sleep);
}
export async function runArrivals({ settings, ...ports }) {
  const plan = arrivalPlan(settings);
  const state = {
    startMs: ports.clock(),
    rows: [],
    pending: new Set(),
    peakInFlight: 0,
    errors: [],
  };
  try {
    await schedule(plan, state, ports);
  } finally {
    await Promise.all(state.pending);
  }
  if (state.errors.length) throw state.errors[0];
  return {
    rows: state.rows,
    wallMs: ports.clock() - state.startMs,
    peakInFlight: state.peakInFlight,
  };
}
