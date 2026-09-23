import { resolve, join } from "node:path";
import {
  mkdir,
  writeFile,
  readFile,
  lstat,
  readdir,
  copyFile,
  chmod,
} from "node:fs/promises";
import { randomBytes } from "node:crypto";
import { settings, secretFiles, environment } from "./deployment-plan.mjs";
import { createCertificates } from "./deployment-tls.mjs";
export function stackDirectory(name) {
  if (!/^[a-z][a-z0-9-]{0,31}$/.test(name))
    throw new Error("Invalid stack name.");
  return resolve(".local/stacks", name);
}
export async function setupStack(name, origin, image, command) {
  const directory = stackDirectory(name);
  const selected = (
    await command(
      "docker",
      ["image", "inspect", image, "--format", "{{.Id}}"],
      { capture: true },
    )
  ).stdout.trim();
  const edgeImage = (
    await command(
      "docker",
      ["image", "inspect", "darkhorse-edge:local", "--format", "{{.Id}}"],
      { capture: true },
    )
  ).stdout.trim();
  const value = settings({ name, origin, image: selected, edgeImage });
  try {
    await lstat(directory);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
    return createStack(directory, value, command);
  }
  const existing = await loadStack(name);
  if (
    existing.settings.origin !== value.origin ||
    existing.settings.image !== value.image ||
    existing.settings.edgeImage !== value.edgeImage
  )
    throw new Error(
      "Stack already exists with another origin/image. Existing identity and credentials were preserved.",
    );
  return existing;
}
async function createStack(directory, value, command) {
  process.umask(0o077);
  await mkdir(resolve(".local/stacks"), { recursive: true, mode: 0o700 });
  await mkdir(directory, { mode: 0o700 });
  const secrets = join(directory, "secrets"),
    pki = join(directory, "pki");
  await mkdir(secrets, { mode: 0o700 });
  await mkdir(pki, { mode: 0o700 });
  const values = secretFiles(
    Array.from({ length: 8 }, () => randomBytes(32).toString("hex")),
  );
  values["login-key"] = randomBytes(32).toString("hex");
  for (const [name, bytes] of Object.entries(values)) {
    await writeFile(join(secrets, name), bytes, { flag: "wx", mode: 0o444 });
    await chmod(join(secrets, name), 0o444);
  }
  await createCertificates(pki, value.host, command);
  for (const name of [
    "ca.pem",
    "edge.pem",
    "edge.key",
    "postgres.pem",
    "postgres.key",
    "cache.pem",
    "cache.key",
    "limiter.pem",
    "limiter.key",
  ]) {
    await copyFile(join(pki, name), join(secrets, name));
    await chmod(join(secrets, name), 0o444);
  }
  // Commit the manifest last: interrupted setup never silently rotates credentials.
  await writeFile(
    join(directory, "settings.json"),
    JSON.stringify(value, null, 2) + "\n",
    { flag: "wx", mode: 0o600 },
  );
  return loadStack(value.name);
}
export async function loadStack(name) {
  const directory = stackDirectory(name);
  await privateDirectory(directory);
  const path = join(directory, "settings.json");
  const info = await lstat(path);
  if (!info.isFile() || info.size > 4096 || info.mode & 0o077)
    throw new Error("Owner-only stack manifest required.");
  const raw = JSON.parse(await readFile(path, "utf8"));
  if (raw.version !== 2 || raw.name !== name)
    throw new Error(
      "Incompatible stack manifest; existing files are preserved. Follow docs/compose.md for the offline role transition.",
    );
  const value = settings(raw);
  if (JSON.stringify(raw) !== JSON.stringify(value))
    throw new Error("Unexpected stack manifest fields.");
  await privateDirectory(join(directory, "secrets"));
  for (const file of await readdir(join(directory, "secrets"))) {
    const entry = await lstat(join(directory, "secrets", file));
    if (!entry.isFile() || entry.mode & 0o222 || entry.size > 32768)
      throw new Error("Regular read-only secret files required.");
  }
  const env = { ...process.env };
  for (const key of Object.keys(env))
    if (key.startsWith("DARKHORSE_") || key.startsWith("COMPOSE_"))
      delete env[key];
  return {
    directory,
    settings: value,
    env: { ...env, ...environment(value, directory) },
    args: [
      "compose",
      "--project-name",
      value.project,
      "--file",
      resolve("deploy/compose.yaml"),
    ],
  };
}
async function privateDirectory(path) {
  const info = await lstat(path);
  if (!info.isDirectory() || info.mode & 0o077)
    throw new Error("Owner-only stack directories required.");
}
