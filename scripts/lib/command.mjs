import { spawn } from "node:child_process";

export function run(
  command,
  args,
  {
    env = process.env,
    input,
    capture = false,
    signal,
    acceptFailure = false,
  } = {},
) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      env,
      signal,
      stdio: [
        input === undefined ? "inherit" : "pipe",
        capture ? "pipe" : "inherit",
        capture ? "pipe" : "inherit",
      ],
    });
    let stdout = "";
    let stderr = "";
    child.stdout?.on("data", (data) => {
      stdout += data;
    });
    child.stderr?.on("data", (data) => {
      stderr += data;
    });
    child.on("error", () => reject(new Error(`Cannot run ${command}.`)));
    child.on("close", (code) => {
      if (code === 0 || acceptFailure) resolve({ code, stdout, stderr });
      else reject(new Error(`${command} failed with status ${code}.`));
    });
    if (input !== undefined) {
      child.stdin.on("error", () => {});
      child.stdin.end(input);
    }
  });
}
