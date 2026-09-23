import { createHash } from "node:crypto";
export function settings({ name, origin, image, edgeImage }) {
  if (
    !/^[a-z][a-z0-9-]{0,31}$/.test(name) ||
    ![image, edgeImage].every((v) => /^sha256:[a-f0-9]{64}$/.test(v))
  )
    throw new Error(
      "Use a bounded stack name and a locally resolved immutable image.",
    );
  const url = new URL(origin);
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.pathname !== "/" ||
    url.search ||
    url.hash ||
    !/^(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z][a-z0-9-]*$/.test(
      url.hostname,
    ) ||
    url.hostname.length > 253 ||
    url.origin !== origin
  )
    throw new Error("Use a canonical HTTPS DNS origin without a path.");
  const port = Number(url.port || 443);
  if (port !== 443 && (port < 1024 || port > 65535))
    throw new Error("Use port 443 or a port from 1024 through 65535.");
  return {
    version: 2,
    name,
    project: `darkhorse-stack-${name}`,
    origin,
    host: url.hostname,
    port,
    image,
    edgeImage,
  };
}
export function environment(value, directory) {
  return {
    DARKHORSE_STACK_DIR: directory,
    DARKHORSE_STACK_ORIGIN: value.origin,
    DARKHORSE_STACK_HOST: value.host,
    DARKHORSE_STACK_PORT: String(value.port),
    DARKHORSE_STACK_IMAGE: value.image,
    DARKHORSE_STACK_EDGE_IMAGE: value.edgeImage,
  };
}
export function secretFiles(values) {
  if (
    values.length !== 8 ||
    new Set(values).size !== 8 ||
    values.some((v) => !/^[a-f0-9]{64}$/.test(v))
  )
    throw new Error("Eight distinct random credentials are required.");
  const [root, owner, runtime, cache, limiter, limiterAdmin, wrap, operator] =
    values;
  const hash = (v) => createHash("sha256").update(v).digest("hex");
  return {
    "postgres-password": root,
    "owner-password": owner,
    "operator-password": operator,
    "operator-db": `postgres://darkhorse_operator:${operator}@postgres:5432/darkhorse`,
    "runtime-password": runtime,
    "owner-db": `postgres://darkhorse_owner:${owner}@postgres:5432/darkhorse`,
    "runtime-db": `postgres://darkhorse_runtime:${runtime}@postgres:5432/darkhorse`,
    "cache-url": `rediss://darkhorse-cache:${cache}@cache:6379/0`,
    "limiter-url": `rediss://darkhorse-limiter:${limiter}@limiter:6379/0`,
    "limiter-admin-url": `rediss://operator:${limiterAdmin}@limiter:6379/0`,
    "cache-acl": `user default off\nuser darkhorse-cache on #${hash(cache)} -@all +ping +info +client|setinfo +client|setname\n`,
    "limiter-acl": `user default off\nuser darkhorse-limiter on #${hash(limiter)} -@all +ping +info +client|setinfo +client|setname ~darkhorse:limiter:v1 +evalsha +script|load +time +hget +hmget +hlen +hset +hscan +hdel\nuser operator on #${hash(limiterAdmin)} ~* &* +@all\n`,
    "wrap-key": wrap,
  };
}
export function operatorArgs(command, args) {
  if (
    command === "limiter-inspect" &&
    args.length === 1 &&
    typeof args[0] === "string" &&
    /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(
      args[0],
    ) &&
    args[0].length === 36 &&
    args[0] !== "00000000-0000-0000-0000-000000000000"
  )
    return ["operator", "limiter", "inspect", args[0]];
  if (
    [
      "migrate",
      "limiter-status",
      "limiter-fence",
      "limiter-activate",
      "signing-status",
      "redis-status",
    ].includes(command) &&
    !args.length
  )
    return [command];
  if (
    command === "bootstrap" &&
    (!args.length || (args.length === 1 && args[0] === "--stdin"))
  )
    return [command, ...args];
  const counter = (v) =>
    /^(0|[1-9][0-9]{0,18})$/.test(v) && BigInt(v) <= 9223372036854775807n;
  if (command === "signing-generate" && args.length === 1 && counter(args[0]))
    return [command, ...args];
  if (
    ["signing-activate", "signing-retire"].includes(command) &&
    args.length === 2 &&
    /^[A-Za-z0-9_-]{43}$/.test(args[0]) &&
    counter(args[1])
  )
    return [command, ...args];
  throw new Error(
    "Unsupported operator command or arguments. Credentials use protected files or standard input.",
  );
}

export function operatorWorkload(command, args) {
  operatorArgs(command, args);
  return command === "migrate" ? "migrator" : "operator";
}
