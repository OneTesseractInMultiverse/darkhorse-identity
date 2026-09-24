import { spawn } from "node:child_process";

// Direct native executable only: no shell, descendants, retry or diagnostic echo.
export function runBenchmarkCommand(
  command,
  args,
  { env, input, signal, timeoutMs = 30000 },
) {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(new Error("Benchmark operator interrupted; outcome unknown."));
      return;
    }
    const child = spawn(command, args, {
      env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    const stdout = [];
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
      else if (output) stdout.push(data);
    };
    child.stdout.on("data", (data) => accept(data, true));
    child.stderr.on("data", (data) => accept(data, false));
    child.on("error", () => {
      failure ??= "Benchmark operator could not start.";
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      signal?.removeEventListener("abort", interrupt);
      if (failure || code !== 0)
        reject(
          new Error(
            failure ??
              "Benchmark operator failed; inspect protected audit evidence before retrying.",
          ),
        );
      else resolve(Buffer.concat(stdout).toString("utf8"));
    });
    child.stdin.on("error", () => {});
    child.stdin.end(input);
  });
}
