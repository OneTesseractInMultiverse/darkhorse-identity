import { createHash } from "node:crypto";
const names = [
  "DARKHORSE_REDIS_CACHE_PORT",
  "DARKHORSE_REDIS_LIMITER_PORT",
  "DARKHORSE_REDIS_CACHE_URL",
  "DARKHORSE_REDIS_LIMITER_URL",
  "DARKHORSE_REDIS_CACHE_ADMIN_URL",
  "DARKHORSE_REDIS_LIMITER_ADMIN_URL",
  "DARKHORSE_REDIS_INSECURE",
];

export function contents(secrets, ports = [63791, 63792]) {
  if (
    secrets.length !== 4 ||
    new Set(secrets).size !== 4 ||
    secrets.some((value) => !/^[a-f0-9]{64}$/.test(value)) ||
    ports.length !== 2 ||
    ports[0] === ports[1] ||
    ports.some(
      (value) => !Number.isInteger(value) || value < 1024 || value > 65535,
    )
  )
    throw new Error("Invalid local Redis setup inputs.");
  const [cache, limiter, cacheAdmin, limiterAdmin] = secrets;
  const url = (user, secret, port) =>
    `redis://${user}:${secret}@127.0.0.1:${port}/0`;
  return {
    ".local/redis-cache.acl": acl("darkhorse-cache", cache, cacheAdmin),
    ".local/redis-limiter.acl": acl("darkhorse-limiter", limiter, limiterAdmin),
    ".local/redis.env": [
      ports[0],
      ports[1],
      url("darkhorse-cache", cache, ports[0]),
      url("darkhorse-limiter", limiter, ports[1]),
      url("operator", cacheAdmin, ports[0]),
      url("operator", limiterAdmin, ports[1]),
      "true",
    ]
      .map((value, i) => `${names[i]}=${value}\n`)
      .join(""),
  };
}
function acl(user, secret, operator) {
  const hash = (value) => createHash("sha256").update(value).digest("hex");
  return `user default off\nuser ${user} on #${hash(secret)} -@all +ping +info +client|setinfo +client|setname\nuser operator on #${hash(operator)} ~* &* +@all\n`;
}
export function parseEnvironment(text) {
  if (text.length > 16384)
    throw new Error("Local Redis settings are too long.");
  const entries = text
    .trim()
    .split("\n")
    .map((line) => {
      const index = line.indexOf("=");
      return [line.slice(0, index), line.slice(index + 1)];
    });
  if (
    entries.length !== names.length ||
    new Set(entries.map(([key]) => key)).size !== names.length ||
    entries.some(([key, value]) => !names.includes(key) || !value)
  )
    throw new Error("Invalid local Redis settings file.");
  return Object.fromEntries(entries);
}

export function runtimeEnvironment(values) {
  return Object.fromEntries(
    Object.entries(values).filter(
      ([key]) =>
        ![
          "DARKHORSE_REDIS_CACHE_ADMIN_URL",
          "DARKHORSE_REDIS_LIMITER_ADMIN_URL",
        ].includes(key),
    ),
  );
}
