import { createHash, randomBytes } from "node:crypto";
import { mkdir, open, lstat, readFile, unlink, rename } from "node:fs/promises";
import { resolve } from "node:path";
import { run } from "./lib/command.mjs";
import {
  contents,
  parseEnvironment,
  runtimeEnvironment,
  aclUpdates,
  databaseEnvironment,
} from "./lib/redis-settings.mjs";
const root = resolve(import.meta.dirname, "..");
process.chdir(root);
const files = [
  ".local/redis-cache.acl",
  ".local/redis-limiter.acl",
  ".local/redis.env",
];
const project = `darkhorse-redis-${createHash("sha256").update(root).digest("hex").slice(0, 10)}`;
async function exists(path) {
  try {
    const stat = await lstat(path);
    if (!stat.isFile() || (stat.mode & 0o077) !== 0)
      throw new Error("Redis settings must be regular owner-only files.");
    return true;
  } catch (error) {
    if (error.code === "ENOENT") return false;
    throw error;
  }
}
async function setup() {
  await mkdir(".local", { recursive: true, mode: 0o700 });
  const present = await Promise.all(files.map(exists));
  if (present.every(Boolean)) return;
  if (present.some(Boolean))
    throw new Error(
      "Redis setup is incomplete; restore the missing files before proceeding.",
    );
  const values = contents(
    Array.from({ length: 4 }, () => randomBytes(32).toString("hex")),
  );
  const created = [];
  try {
    for (const [path, content] of Object.entries(values)) {
      const handle = await open(path, "wx", 0o600);
      created.push(path);
      try {
        await handle.writeFile(content);
      } finally {
        await handle.close();
      }
    }
  } catch (error) {
    for (const path of created) await unlink(path);
    throw error;
  }
}
async function environment() {
  for (const file of files)
    if (!(await exists(file))) throw new Error("Run make redis-setup first.");
  return {
    ...process.env,
    ...parseEnvironment(await readFile(files[2], "utf8")),
  };
}
async function main() {
  const [operation, ...extra] = process.argv.slice(2);
  if (extra.length)
    throw new Error(
      "Use setup, up, down, status, acl-update, limiter-status, limiter-fence, or limiter-activate.",
    );
  if (operation === "setup") {
    await setup();
    console.log("Redis credentials prepared; existing files preserved.");
    return;
  }
  if (operation === "up") await setup();
  const env = await environment();
  if (operation === "acl-update") {
    for (const [path, content] of Object.entries(aclUpdates(env))) {
      const temporary = `${path}.new`;
      const handle = await open(temporary, "wx", 0o600);
      try {
        await handle.writeFile(content);
      } finally {
        await handle.close();
      }
      await rename(temporary, path);
    }
    console.log(
      "ACL policy updated with existing credentials. Restart both local Redis services to load it.",
    );
    return;
  }
  if (
    ["limiter-fence", "limiter-activate", "limiter-status"].includes(operation)
  ) {
    if (!(await exists(".local/database.env")))
      throw new Error("Run make db-setup first.");
    const runtime = {
      ...runtimeEnvironment(env),
      ...databaseEnvironment(await readFile(".local/database.env", "utf8")),
    };
    if (operation === "limiter-activate")
      runtime.DARKHORSE_REDIS_LIMITER_ADMIN_URL =
        env.DARKHORSE_REDIS_LIMITER_ADMIN_URL;
    await run(
      "cargo",
      [
        "run",
        "--locked",
        "--offline",
        "-p",
        "darkhorse-server",
        "--",
        operation,
      ],
      { env: runtime },
    );
    return;
  }
  if (operation === "status")
    await run(
      "cargo",
      [
        "run",
        "--locked",
        "--offline",
        "-p",
        "darkhorse-server",
        "--",
        "redis-status",
      ],
      { env: runtimeEnvironment(env) },
    );
  else if (operation === "up" || operation === "down")
    await run(
      "docker",
      [
        "compose",
        "--project-name",
        project,
        "--env-file",
        files[2],
        "--file",
        "config/compose.redis.yaml",
        ...(operation === "up" ? ["up", "--detach", "--wait"] : ["down"]),
      ],
      { env },
    );
  else throw new Error("Use setup, up, down, or status.");
}
main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
