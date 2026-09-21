import { resolve } from "node:path";
import { setupStack, loadStack } from "./lib/deployment-state.mjs";
import {
  compose,
  operator,
  migrate,
  check,
  backup,
} from "./lib/deployment-operations.mjs";
import { run } from "./lib/command.mjs";
process.chdir(resolve(import.meta.dirname, ".."));
async function main() {
  const [mode, name, ...args] = process.argv.slice(2);
  if (mode === "setup") {
    if (args.length !== 2)
      throw new Error(
        "Setup needs a stack name, canonical HTTPS origin and local image.",
      );
    const s = await setupStack(name, args[0], args[1], run);
    console.log(
      `Stack ${s.settings.name} is prepared at ${s.directory}. Local test certificates expire in seven days; no host trust was changed.`,
    );
    return;
  }
  const stack = await loadStack(name);
  if (mode === "infra")
    await compose(stack, [
      "up",
      "--detach",
      "--wait",
      "--wait-timeout",
      "90",
      "postgres",
      "cache",
      "limiter",
    ]);
  else if (mode === "migrate") await migrate(stack);
  else if (mode === "up")
    await compose(stack, [
      "up",
      "--detach",
      "--wait",
      "--wait-timeout",
      "60",
      "api",
      "edge",
    ]);
  else if (mode === "stop") await compose(stack, ["stop", "edge", "api"]);
  else if (mode === "down") await compose(stack, ["down"]);
  else if (mode === "status") await compose(stack, ["ps"]);
  else if (mode === "check") await check(stack);
  else if (mode === "backup") await backup(stack);
  else if (mode === "operator") await operator(stack, args[0], args.slice(1));
  else
    throw new Error(
      "Use setup, infra, migrate, up, stop, down, status, check, backup or operator.",
    );
}
main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
