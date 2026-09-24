import { transport } from "./account-transport.mjs";
// Control commands receive only manifest data. Authentication stdin is reserved
// for the separate exec transport and is never captured by this coordinator.
export async function accountControl(spec, signal) {
  const output = [],
    abort = new AbortController();
  let bytes = 0;
  const result = await transport(
    {
      ...spec,
      input: spec.input ?? "",
      stdout: (chunk) => {
        bytes += chunk.length;
        if (bytes > 1048576) abort.abort("output_limit");
        else output.push(chunk);
      },
      stderr: () => {},
    },
    35000,
    signal ? AbortSignal.any([signal, abort.signal]) : abort.signal,
  );
  if (result.code !== 0 || result.uncertain || abort.signal.aborted)
    throw new Error("Kubernetes account control operation failed.");
  return Buffer.concat(output).toString("utf8");
}
