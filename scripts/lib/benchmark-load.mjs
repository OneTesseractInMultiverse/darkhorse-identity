import { classify, validateLoad } from "./benchmark-model.mjs";

const availabilityResponseClasses = new Set([
  "expected_error_json_503",
  "other_503_body",
  "expected_error_json_429",
  "other_429_body",
]);

export async function attempt(index, selection, perform, clock) {
  const startMs = clock();
  let response;
  try {
    response = await perform(selection, index);
  } catch {
    response = { status: 0 };
  }
  const endMs = clock();
  const availabilityResponseClass = availabilityResponseClasses.has(
    response.availabilityResponseClass,
  )
    ? response.availabilityResponseClass
    : undefined;
  return {
    index,
    client: selection.client,
    epoch: selection.epoch,
    startMs,
    elapsedMs: endMs - startMs,
    status: response.status,
    outcome: classify(response, selection.expected),
    ...(availabilityResponseClass === undefined
      ? {}
      : { availabilityResponseClass }),
  };
}
export async function runLoad({ count, concurrency, select, perform, clock }) {
  validateLoad(count, concurrency);
  const start = clock(),
    rows = [];
  let next = 0;
  async function worker() {
    while (next < count) {
      const index = next++;
      const selection = select(index);
      rows[index] = await attempt(index, selection, perform, clock);
    }
  }
  const workers = await Promise.allSettled(
    Array.from({ length: Math.min(count, concurrency) }, worker),
  );
  const failure = workers.find((result) => result.status === "rejected");
  if (failure) throw failure.reason;
  return { rows, wallMs: clock() - start };
}
