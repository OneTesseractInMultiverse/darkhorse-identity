import { startProcess } from "./process.mjs";
import { superviseAccount } from "./account-supervision.mjs";
// Stops the owned local process group only. Remote execution may outlive a lost
// attachment; the caller must inspect state, never interpret this as rollback.
export async function transport(spec, timeoutMs = 120000, signal) {
  const abort = new AbortController();
  const inherited = () => abort.abort(signal.reason);
  signal?.addEventListener("abort", inherited, { once: true });
  if (signal?.aborted) inherited();
  const handlers = ["SIGINT", "SIGTERM", "SIGHUP"].map((name) => [
    name,
    () => abort.abort(name),
  ]);
  let timer;
  const deadline = new Promise((resolve) => {
    timer = setTimeout(resolve, timeoutMs);
  });
  for (const [name, handler] of handlers) process.once(name, handler);
  try {
    return await superviseAccount(spec, startProcess, abort.signal, deadline);
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener("abort", inherited);
    for (const [name, handler] of handlers)
      process.removeListener(name, handler);
  }
}
