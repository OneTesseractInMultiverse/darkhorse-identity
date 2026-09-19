import { MAX_FRAME, parseProfile } from "./benchmark-profile-model.mjs";
const prefix = "DARKHORSE_PROFILE ";
// Transport belongs only to a disposable child; never signal a supplied PID.
export function profileChannel({
  schedule = setTimeout,
  cancel = clearTimeout,
} = {}) {
  let buffer = "",
    pending,
    failure;
  function fail(message) {
    failure ??= new Error(message);
    if (pending) {
      cancel(pending.timer);
      pending.reject(failure);
      pending = undefined;
    }
  }
  function accept(chunk) {
    if (failure) return;
    buffer += chunk.toString();
    while (buffer.includes("\n")) {
      const end = buffer.indexOf("\n");
      const line = buffer.slice(0, end);
      buffer = buffer.slice(end + 1);
      if (line.length > MAX_FRAME)
        return fail("Benchmark profile frame too large.");
      if (!line.startsWith(prefix)) continue;
      if (!pending) return fail("Received unsolicited benchmark profile.");
      try {
        const report = parseProfile(line.slice(prefix.length));
        cancel(pending.timer);
        pending.resolve(report);
        pending = undefined;
      } catch {
        return fail("Invalid benchmark profile frame.");
      }
    }
    if (buffer.length > MAX_FRAME) fail("Benchmark profile frame too large.");
  }
  async function request(signal) {
    if (failure) throw failure;
    if (pending) throw new Error("Benchmark profile already pending.");
    return new Promise((resolve, reject) => {
      pending = {
        resolve,
        reject,
        timer: schedule(() => fail("Benchmark profile timed out."), 5000),
      };
      try {
        if (!signal()) fail("Cannot signal benchmark process.");
      } catch {
        fail("Cannot signal benchmark process.");
      }
    });
  }
  return {
    accept,
    request,
    close: () => fail("Benchmark process exited before profile collection."),
  };
}
