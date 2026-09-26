import { startProcess } from "./process.mjs";

// Keep raw child output private and bounded; the caller publishes typed counts only.
export async function boundaryProcess(
  command,
  args,
  { env, signal, timeoutMs = 25 * 60_000, maxBytes = 2 * 1024 * 1024 } = {},
) {
  if (signal?.aborted)
    return { code: null, stdout: "", interrupted: true, overflow: false };
  let bytes = 0,
    stdout = "",
    overflow = false,
    interrupted = false,
    stopping;
  let child;
  function stop() {
    stopping ??= child?.stop();
  }
  function capture(data, keep) {
    bytes += data.length;
    if (bytes > maxBytes) {
      overflow = true;
      stop();
    } else if (keep) stdout += data.toString();
  }
  function interrupt() {
    interrupted = true;
    stop();
  }
  child = startProcess({
    command,
    args,
    env,
    input: "",
    stdout: (data) => capture(data, true),
    stderr: (data) => capture(data, false),
  });
  signal?.addEventListener("abort", interrupt, { once: true });
  const timer = setTimeout(interrupt, timeoutMs);
  timer.unref();
  try {
    const result = await child.done.catch(() => ({ code: null }));
    await stopping;
    return { code: result.code, stdout, interrupted, overflow };
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener("abort", interrupt);
  }
}
