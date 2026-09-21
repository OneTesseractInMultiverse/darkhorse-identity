import { execFile } from "node:child_process";
// Bounded output and runtime; a failed scanner retains its exit status, never a clean result.
export function execute(
  file,
  args,
  { cwd = process.cwd(), input, timeout = 300000, encoding = "utf8" } = {},
) {
  return new Promise((resolve) => {
    const child = execFile(
      file,
      args,
      {
        cwd,
        timeout,
        killSignal: "SIGKILL",
        maxBuffer: 32 * 1024 * 1024,
        encoding,
      },
      (error, stdout, stderr) =>
        resolve({
          code: error ? (Number.isInteger(error.code) ? error.code : null) : 0,
          stdout,
          stderr,
        }),
    );
    child.stdin.on("error", () => {});
    child.stdin.end(input);
  });
}
export async function successful(file, args, options) {
  const result = await execute(file, args, options);
  if (result.code !== 0)
    throw new Error("Required verification command failed.");
  return result.stdout;
}
