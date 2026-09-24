import { resolve } from "node:path";
import { readFile, stat } from "node:fs/promises";
import { randomBytes } from "node:crypto";
import { loadStack } from "./deployment-state.mjs";
import { configuration } from "./kubernetes-plan.mjs";
import {
  composeAccount,
  kubernetesAccount,
  UsageError,
} from "./account-plan.mjs";
import { transport } from "./account-transport.mjs";
import { kubernetesAccountRun } from "./kubernetes-account-run.mjs";
async function select(mode, args, values) {
  if (mode === "kube-exec" || mode === "kube-run") {
    const file = values.KUBE_CONFIG;
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
      args: kubernetesAccount(c.namespace, values, args),
    };
  }
  const stack = await loadStack(values.STACK ?? "local");
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
async function main(options) {
  const [mode, ...extra] = process.argv.slice(2);
  if (
    extra.length ||
    !["compose-exec", "compose-run", "kube-exec", "kube-run"].includes(mode)
  )
    throw new UsageError(
      "Use compose-exec, compose-run, kube-exec or kube-run; nonsecret settings are environment variables. See docs/container-accounts.md.",
    );
  const args = options(process.env, Boolean(process.stdin.isTTY));
  const selected = await select(mode, args, process.env);
  const result =
    mode === "kube-run"
      ? await kubernetesAccountRun(selected.configuration, process.env, args)
      : await transport(selected);
  if (result.uncertain)
    console.error(
      "Administration transport interrupted or timed out. Remote execution may continue or have committed. Inspect the operation audit and applicable target state before any retry; no retry was attempted.",
    );
  process.exitCode = result.code;
}
export async function launch(options) {
  process.chdir(resolve(import.meta.dirname, "../.."));
  await main(options).catch((error) => {
    console.error(
      error instanceof UsageError
        ? error.message
        : "Cannot launch administration command. Check the selected deployment and installed tools. No retry was attempted; inspect remote state if execution may have started.",
    );
    process.exitCode = error instanceof UsageError ? 2 : 1;
  });
}
