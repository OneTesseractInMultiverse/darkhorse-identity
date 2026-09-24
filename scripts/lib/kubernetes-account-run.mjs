import { randomBytes } from "node:crypto";
import { accountControl } from "./account-control.mjs";
import { transport } from "./account-transport.mjs";
import {
  kubernetesAccess,
  kubernetesAccount,
  interrupted,
} from "./account-plan.mjs";
import { accountPod, removal } from "./kubernetes-account-plan.mjs";
import { runAccountPod } from "./kubernetes-account-lifecycle.mjs";
export async function kubernetesAccountRun(c, values, args) {
  const name = `darkhorse-account-${randomBytes(8).toString("hex")}`;
  const expected = accountPod(c, name);
  const execArgs = kubernetesAccount(
    c.namespace,
    { ...values, ACCOUNT_POD: name },
    args,
  );
  const base = kubernetesAccess(c.namespace, values);
  const abort = new AbortController();
  const handlers = ["SIGINT", "SIGTERM", "SIGHUP"].map((signal) => [
    signal,
    () => abort.abort(signal),
  ]);
  for (const [signal, handler] of handlers) process.once(signal, handler);
  console.error(`One-shot account Pod: ${c.namespace}/${name}`);
  const effects = kubernetesEffects(base, expected, execArgs, abort.signal);
  try {
    return await runAccountPod(expected, effects);
  } catch (error) {
    if (abort.signal.aborted) return interrupted(abort.signal.reason);
    throw error;
  } finally {
    for (const [signal, handler] of handlers)
      process.removeListener(signal, handler);
  }
}
function kubernetesEffects(base, expected, execArgs, signal) {
  const command = (args, input = "", cleanup = false) =>
    accountControl(
      { command: "kubectl", args: [...base, ...args], input },
      cleanup ? undefined : signal,
    );
  const resource = `pod/${expected.metadata.name}`;
  return {
    create: async (manifest) => {
      const result = await command(
        ["create", "-f", "-", "-o", "json"],
        JSON.stringify(manifest),
      );
      const created = JSON.parse(result);
      return created;
    },
    ready: () =>
      command(["wait", "--for=condition=Ready", resource, "--timeout=25s"]),
    inspect: async () =>
      JSON.parse(await command(["get", resource, "-o", "json"])),
    execute: () => {
      signal.throwIfAborted();
      return transport({ command: "kubectl", args: execArgs });
    },
    remove: async (uid) => {
      const request = removal(expected, uid);
      await command(
        ["delete", "--raw", request.path, "-f", "-"],
        JSON.stringify(request.body),
        true,
      );
      await command(
        ["wait", "--for=delete", resource, "--timeout=15s"],
        "",
        true,
      );
    },
    report: (message) => console.error(message),
  };
}
