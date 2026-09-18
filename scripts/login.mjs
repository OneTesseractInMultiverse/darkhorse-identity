import { randomBytes } from "node:crypto";
import { mkdir, readFile, writeFile, stat } from "node:fs/promises";
import { run } from "./lib/command.mjs";
import {
  databaseEnvironment,
  parseEnvironment,
  runtimeEnvironment,
} from "./lib/redis-settings.mjs";

const mode = process.argv[2];
async function privateRead(path) {
  if ((await stat(path)).mode & 0o077)
    throw new Error(
      "Local credential files must be accessible only by their owner.",
    );
  return readFile(path, "utf8");
}
async function main() {
  if (mode === "setup") {
    process.umask(0o077);
    await mkdir(".local", { recursive: true, mode: 0o700 });
    try {
      await writeFile(".local/login.key", randomBytes(32).toString("hex"), {
        flag: "wx",
        mode: 0o600,
      });
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
    }
    console.log("Local login key is ready; existing key preserved.");
    return;
  }
  if (mode !== "dev") throw new Error("Usage: login.mjs setup|dev");
  const key = (await privateRead(".local/login.key")).trim();
  if (!/^[a-f0-9]{64}$/.test(key)) throw new Error("Invalid local login key.");
  const database = databaseEnvironment(
    await privateRead(".local/database.env"),
  );
  const redis = runtimeEnvironment(
    parseEnvironment(await privateRead(".local/redis.env")),
  );
  const env = runtimeEnvironment({
    ...process.env,
    ...database,
    ...redis,
    DARKHORSE_LOGIN_ENABLED: "true",
    DARKHORSE_LOGIN_LIMIT_KEY: key,
  });
  await run(process.execPath, ["scripts/dev.mjs", "all"], { env });
}
try {
  await main();
} catch {
  console.error(
    "Login development could not start. Check local setup, private file permissions and service availability.",
  );
  process.exitCode = 1;
}
