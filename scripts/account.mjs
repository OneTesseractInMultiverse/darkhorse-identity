import { resolve } from "node:path";
import { readFile, stat } from "node:fs/promises";
import { randomBytes } from "node:crypto";
import { loadStack } from "./lib/deployment-state.mjs";
import { configuration } from "./lib/kubernetes-plan.mjs";
import {
  accountOptions,
  composeAccount,
  kubernetesAccount,
  UsageError,
} from "./lib/account-plan.mjs";
import { transport } from "./lib/account-transport.mjs";
import { kubernetesAccountRun } from "./lib/kubernetes-account-run.mjs";
process.chdir(resolve(import.meta.dirname, ".."));
async function select(mode, args) {
  if (mode === "kube-exec" || mode === "kube-run") {
    const file = process.env.KUBE_CONFIG;
    if (!file?.startsWith("/"))
      throw new UsageError(
        "Supply an absolute KUBE_CONFIG plus KUBE_ACCESS and KUBE_CONTEXT. kube-exec also requires ACCOUNT_POD.",
      );
    const info = await stat(file);
    if (!info.isFile() || info.size > 16384)
      throw new UsageError(
        "A bounded regular Kubernetes configuration file is required.",
      );
    const c = configuration(JSON.parse(await readFile(file, "utf8")));
    if (mode === "kube-run") return { configuration: c };
    return {
      command: "kubectl",
      args: kubernetesAccount(c.namespace, process.env, args),
    };
  }
  const stack = await loadStack(process.env.STACK ?? "local");
  const name =
    mode === "compose-run"
      ? `darkhorse-account-${randomBytes(8).toString("hex")}`
      : undefined;
  if (name) console.error(`One-shot account container: ${name}`);
  return {
    command: "docker",
    args: [...stack.args, ...composeAccount(mode, args, name)],
    env: stack.env,
  };
}
async function main() {
  const [mode, ...extra] = process.argv.slice(2);
  if (
    extra.length ||
    !["compose-exec", "compose-run", "kube-exec", "kube-run"].includes(mode)
  )
    throw new UsageError(
      "Use compose-exec, compose-run, kube-exec or kube-run; nonsecret settings are environment variables. See docs/container-accounts.md.",
    );
  const args = accountOptions(process.env, Boolean(process.stdin.isTTY));
  const selected = await select(mode, args);
  const result =
    mode === "kube-run"
      ? await kubernetesAccountRun(selected.configuration, process.env, args)
      : await transport(selected);
  if (result.uncertain)
    console.error(
      "Account transport interrupted or timed out. Remote execution may continue or have committed. Inspect the target, current revision and operation audit before any retry; no retry was attempted.",
    );
  process.exitCode = result.code;
}
main().catch((error) => {
  console.error(
    error instanceof UsageError
      ? error.message
      : "Cannot launch account command. Check the selected deployment and installed tools. No retry was attempted; inspect remote state if execution may have started.",
  );
  process.exitCode = error instanceof UsageError ? 2 : 1;
});
