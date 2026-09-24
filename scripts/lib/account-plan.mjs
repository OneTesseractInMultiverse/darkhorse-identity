export class UsageError extends Error {}
const invalid = () =>
  new UsageError(
    "Use a supported account operation, nonzero UUID, expected revision for changes and ACCOUNT_CONFIRM=yes only for explicit confirmation. See docs/container-accounts.md.",
  );
export function accountOptions(values, stdinIsTTY) {
  if (stdinIsTTY)
    throw new UsageError(
      "Account launchers require protected stdin; use a protected file or pipe. Use the documented native terminal commands for hidden interactive input.",
    );
  const operation = values.ACCOUNT_OPERATION ?? "show",
    id = values.ACCOUNT_ID ?? "",
    revision = values.ACCOUNT_REVISION ?? "",
    confirm = values.ACCOUNT_CONFIRM ?? "no";
  if (
    !["show", "deactivate", "reactivate", "revoke-all"].includes(operation) ||
    !/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i.test(id) ||
    id === "00000000-0000-0000-0000-000000000000" ||
    !["yes", "no"].includes(confirm)
  )
    throw invalid();
  if (
    operation === "show"
      ? revision !== ""
      : !/^(0|[1-9][0-9]{0,18})$/.test(revision) ||
        BigInt(revision) > 9223372036854775807n
  )
    throw invalid();
  return [
    "--auth-stdin",
    "--output",
    "json",
    ...(confirm === "yes" ? ["--yes"] : []),
    "operator",
    "account",
    operation,
    id,
    ...(operation === "show" ? [] : [revision]),
  ];
}
export function composeAccount(mode, args, name) {
  if (mode === "compose-exec")
    return [
      "exec",
      "-T",
      "--user",
      "10001:10001",
      "api",
      "/usr/local/bin/darkhorse-server",
      ...args,
    ];
  if (
    mode !== "compose-run" ||
    !/^darkhorse-account-[a-f0-9]{16}$/.test(name ?? "")
  )
    throw invalid();
  return ["run", "--rm", "--no-deps", "-T", "--name", name, "account", ...args];
}
export function kubernetesAccess(namespace, values) {
  const { KUBE_ACCESS: access, KUBE_CONTEXT: context } = values;
  if (
    !access?.startsWith("/") ||
    !context ||
    [access, context].some((v) => v.length > 4096 || /[\x00-\x1f\x7f]/.test(v))
  )
    throw new UsageError(
      "Kubernetes account execution requires an absolute KUBE_ACCESS and explicit KUBE_CONTEXT.",
    );
  return [
    "--kubeconfig",
    access,
    "--context",
    context,
    "--namespace",
    namespace,
    "--request-timeout=30s",
  ];
}
export function kubernetesAccount(namespace, values, args) {
  const pod = values.ACCOUNT_POD;
  if (
    !/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)*$/.test(
      pod ?? "",
    ) ||
    pod.length > 253
  )
    throw new UsageError(
      "Kubernetes execution requires a single ACCOUNT_POD name.",
    );
  return [
    ...kubernetesAccess(namespace, values),
    "exec",
    "--pod-running-timeout=15s",
    "-i",
    `pod/${pod}`,
    "--container",
    "api",
    "--",
    "/usr/local/bin/darkhorse-server",
    ...args,
  ];
}
export function exitCode({ code }) {
  return Number.isInteger(code) && code >= 0 && code <= 255 ? code : 1;
}
export function interrupted(reason) {
  return {
    code: reason === "SIGTERM" ? 143 : reason === "SIGHUP" ? 129 : 130,
    uncertain: true,
  };
}
