import { readFile, stat } from "node:fs/promises";
import { resolve } from "node:path";
import {
  configuration,
  application,
  operatorJob,
  budgets,
} from "./lib/kubernetes-plan.mjs";
import { run } from "./lib/command.mjs";
import { backendPolicies } from "./lib/kubernetes-network.mjs";
process.chdir(resolve(import.meta.dirname, ".."));
async function main() {
  const [mode, path, ...args] = process.argv.slice(2);
  if (!path)
    throw new Error(
      "Provide KUBE_CONFIG with the documented nonsecret deployment configuration.",
    );
  const info = await stat(path);
  if (!info.isFile() || info.size > 16384)
    throw new Error("A bounded regular configuration file is required.");
  const c = configuration(JSON.parse(await readFile(path, "utf8"))),
    manifest = application(c);
  if (mode === "render" && !args.length)
    return console.log(JSON.stringify(manifest, null, 2));
  if (mode === "backend-render" && !args.length)
    return console.log(
      JSON.stringify(
        { apiVersion: "v1", kind: "List", items: backendPolicies(c) },
        null,
        2,
      ),
    );
  if (mode === "budgets" && !args.length)
    return console.log(JSON.stringify(budgets(), null, 2));
  if (mode === "job" && (args.length === 2 || args.length === 3))
    return console.log(
      JSON.stringify(operatorJob(c, args[0], args[1], args.slice(2)), null, 2),
    );
  if (
    !["prepare", "validate", "apply", "status"].includes(mode) ||
    args.length !== 2 ||
    !args[0] ||
    !args[1] ||
    args.some((x) => x.length > 4096 || /[\x00-\x1f\x7f]/.test(x))
  )
    throw new Error(
      "Cluster commands require an explicit kubeconfig path and context. No ambient cluster is selected.",
    );
  const base = [
    "--kubeconfig",
    resolve(args[0]),
    "--context",
    args[1],
    "--request-timeout=15s",
    "--namespace",
    c.namespace,
  ];
  const invoke = (argv, options = {}) =>
    run("kubectl", [...base, ...argv], options);
  if (mode === "status")
    return invoke(["get", "deployment,pods,service,jobs,resourcequota"]);
  const namespace = (
    await invoke(
      ["get", "namespace", c.namespace, "--ignore-not-found", "-o", "json"],
      { capture: true },
    )
  ).stdout.trim();
  if (
    namespace &&
    JSON.parse(namespace).metadata.labels?.["app.kubernetes.io/managed-by"] !==
      "darkhorse"
  )
    throw new Error("Refusing to adopt an existing namespace.");
  if (mode === "prepare")
    return invoke(["apply", "-f", "-"], {
      input: JSON.stringify({
        ...manifest,
        items: manifest.items.filter(
          (x) =>
            !["Deployment", "Service", "PodDisruptionBudget"].includes(x.kind),
        ),
      }),
    });
  await invoke(["apply", "--dry-run=server", "-f", "-"], {
    input: JSON.stringify(manifest),
  });
  if (mode === "apply")
    await invoke(["apply", "-f", "-"], { input: JSON.stringify(manifest) });
}
main().catch(() => {
  console.error(
    "Kubernetes operation failed. Check the explicit configuration/context, dependencies and server diagnostics; do not retry uncertain mutations automatically.",
  );
  process.exitCode = 1;
});
