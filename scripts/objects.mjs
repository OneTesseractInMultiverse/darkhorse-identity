import { provisionBucket } from "./lib/objects-provision.mjs";
import { randomBytes } from "node:crypto";
import { mkdir, lstat, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { run } from "./lib/command.mjs";
import {
  objectSettings,
  objectEnvironment,
  objectProject,
} from "./lib/objects-settings.mjs";
process.chdir(resolve(import.meta.dirname, ".."));
const path = ".local/objects.json";
async function main() {
  const mode = process.argv[2];
  if (mode === "setup") {
    process.umask(0o077);
    await mkdir(".local", { recursive: true, mode: 0o700 });
    try {
      await writeFile(
        path,
        JSON.stringify({
          access: randomBytes(32).toString("hex"),
          secret: randomBytes(32).toString("hex"),
        }),
        { flag: "wx", mode: 0o600 },
      );
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
    }
    console.log(
      "Local object credentials are ready; existing credentials preserved.",
    );
    return;
  }
  const info = await lstat(path);
  if (!info.isFile() || info.mode & 0o077)
    throw new Error("Owner-only object credentials required.");
  const settings = objectSettings(await readFile(path, "utf8")),
    env = { ...process.env, ...objectEnvironment(settings) };
  const args = [
    "compose",
    "--project-name",
    objectProject(process.cwd()),
    "--file",
    "config/compose.objects.yaml",
  ];
  if (mode === "dev") {
    await run(process.execPath, ["scripts/login.mjs", "dev"], { env });
    return;
  }
  if (mode === "down") {
    await run("docker", [...args, "down"], { env });
    return;
  }
  if (mode !== "up") throw new Error("Use setup, up, down or dev.");
  await run("docker", [...args, "up", "--detach"], { env });
  for (let i = 0; i < 120; i++) {
    try {
      const r = await fetch(`${env.DARKHORSE_OBJECTS_ENDPOINT}/health`, {
        signal: AbortSignal.timeout(500),
      });
      if (r.status < 500) break;
    } catch {}
    if (i === 119) throw new Error("Object service did not become ready.");
    await delay(250);
  }
  const endpoint = env.DARKHORSE_OBJECTS_ENDPOINT,
    bucket = env.DARKHORSE_OBJECTS_BUCKET;
  await provisionBucket(
    run,
    endpoint,
    bucket,
    settings.access,
    settings.secret,
  );
  console.log("Private local image storage is ready on loopback port 9009.");
}
main().catch(() => {
  console.error(
    "Object storage operation failed. Check Docker, private settings and loopback port 9009.",
  );
  process.exitCode = 1;
});
