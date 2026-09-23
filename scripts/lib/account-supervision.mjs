import { exitCode, interrupted } from "./account-plan.mjs";
export async function superviseAccount(spec, start, signal, deadline) {
  if (signal.aborted) return interrupted(signal.reason);
  const child = start(spec);
  let onAbort;
  const cancellation = new Promise((resolve) => {
    onAbort = () => resolve(interrupted(signal.reason));
    signal.addEventListener("abort", onAbort, { once: true });
    if (signal.aborted) onAbort();
  });
  let result;
  try {
    result = await Promise.race([
      child.done.then((value) => ({
        code: exitCode(value),
        uncertain: value.code === null,
      })),
      cancellation,
      deadline.then(() => ({ code: 124, uncertain: true })),
    ]);
  } catch (error) {
    await child.stop();
    throw error;
  } finally {
    signal.removeEventListener("abort", onAbort);
  }
  if (result.uncertain) await child.stop();
  return result;
}
