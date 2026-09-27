import { spawn } from "node:child_process";

// Direct native executable only: no shell, descendants, retry or diagnostic echo.
export function runBenchmarkCommand(
  command,
  args,
  { env, input, signal, timeoutMs = 30000, expectedExitCodes = [0] },
) {
  return new Promise((resolve, reject) => {
    if (
      !Array.isArray(expectedExitCodes) ||
      expectedExitCodes.length < 1 ||
      expectedExitCodes.length > 2 ||
      !expectedExitCodes.includes(0) ||
      new Set(expectedExitCodes).size !== expectedExitCodes.length ||
      expectedExitCodes.some(
        (code) => !Number.isInteger(code) || code < 0 || code > 255,
      )
    ) {
      reject(new Error("Invalid benchmark command exit policy."));
      return;
    }
    if (signal?.aborted) {
      reject(new Error("Benchmark operator interrupted; outcome unknown."));
      return;
    }
    const child = spawn(command, args, {
      env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    const stdout = [],
      stderr = [];
    let bytes = 0,
      failure;
    const stop = (message) => {
      failure ??= message;
      child.kill("SIGKILL");
    };
    const interrupt = () =>
      stop("Benchmark operator interrupted; outcome unknown.");
    const timer = setTimeout(
      () => stop("Benchmark operator deadline exceeded; outcome unknown."),
      timeoutMs,
    );
    signal?.addEventListener("abort", interrupt, { once: true });
    const accept = (data, output) => {
      bytes += data.length;
      if (bytes > 65536)
        stop("Benchmark operator output limit exceeded; outcome unknown.");
      else output.push(data);
    };
    child.stdout.on("data", (data) => accept(data, stdout));
    child.stderr.on("data", (data) => accept(data, stderr));
    child.on("error", () => {
      failure ??= "Benchmark operator could not start.";
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      signal?.removeEventListener("abort", interrupt);
      if (
        !failure &&
        code !== 0 &&
        expectedExitCodes.includes(code) &&
        stdout.length > 0
      ) {
        reject(
          new Error("Benchmark operator wrote to an unexpected output stream."),
        );
      } else if (failure || !expectedExitCodes.includes(code))
        reject(
          new Error(
            failure ??
              "Benchmark operator failed; inspect protected audit evidence before retrying.",
          ),
        );
      else
        resolve(Buffer.concat(code === 0 ? stdout : stderr).toString("utf8"));
    });
    child.stdin.on("error", () => {});
    child.stdin.end(input);
  });
}
