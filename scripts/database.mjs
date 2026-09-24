import { createHash, randomBytes } from "node:crypto";
import { mkdir, open, readFile, lstat, unlink } from "node:fs/promises";
import { resolve } from "node:path";
import { run } from "./lib/command.mjs";
import { operatorArgs } from "./lib/deployment-plan.mjs";
import { checkDatabaseVolumes } from "./lib/database-stack.mjs";

const root = resolve(import.meta.dirname, "..");
process.chdir(root);
const files = [".local/database-password", ".local/database.env"];
const project = `darkhorse-${createHash("sha256").update(root).digest("hex").slice(0, 10)}`;
const compose = [
  "compose",
  "--project-name",
  project,
  "--env-file",
  files[1],
  "--file",
  "config/compose.dev.yaml",
];

async function fileExists(path) {
  try {
    const info = await lstat(path);
    if (!info.isFile() || (info.mode & 0o077) !== 0)
      throw new Error(
        "Local database secrets must be regular owner-only files.",
      );
    return true;
  } catch (error) {
    if (error.code === "ENOENT") return false;
    throw error;
  }
}

async function setup() {
  await mkdir(".local", { recursive: true, mode: 0o700 });
  const existing = await Promise.all(files.map(fileExists));
  if (existing.every(Boolean)) return;
  if (existing.some(Boolean))
    throw new Error(
      "Local database credentials are incomplete; restore the missing file before proceeding.",
    );
  const password = randomBytes(32).toString("hex");
  const contents = [
    password + "\n",
    `DARKHORSE_DATABASE_PORT=54329\nDARKHORSE_DATABASE_URL=postgres://darkhorse:${password}@127.0.0.1:54329/darkhorse\nDARKHORSE_DATABASE_INSECURE=true\n`,
  ];
  const created = [];
  try {
    for (let index = 0; index < files.length; index++) {
      const handle = await open(files[index], "wx", 0o600);
      created.push(files[index]);
      try {
        await handle.writeFile(contents[index]);
      } finally {
        await handle.close();
      }
    }
  } catch (error) {
    for (const file of created) await unlink(file);
    throw error;
  }
}

async function environment() {
  for (const file of files)
    if (!(await fileExists(file))) throw new Error("Run make db-setup first.");
  const entries = (await readFile(files[1], "utf8"))
    .trim()
    .split("\n")
    .map((line) => {
      const index = line.indexOf("=");
      const key = line.slice(0, index);
      if (
        ![
          "DARKHORSE_DATABASE_PORT",
          "DARKHORSE_DATABASE_URL",
          "DARKHORSE_DATABASE_INSECURE",
        ].includes(key)
      )
        throw new Error("Invalid local database settings file.");
      return [key, line.slice(index + 1)];
    });
  return { ...process.env, ...Object.fromEntries(entries) };
}

async function main() {
  const [operation, ...args] = process.argv.slice(2);
  if (operation === "setup") {
    await setup();
    console.log(
      "Local database credentials prepared; existing secrets preserved.",
    );
  } else if (operation === "up") {
    await setup();
    const volumes = await run(
      "docker",
      ["volume", "ls", "--format", "{{.Name}}"],
      { capture: true },
    );
    checkDatabaseVolumes(volumes.stdout.trim().split("\n"), project);
    await run("docker", [...compose, "up", "--detach", "--wait"], {
      env: await environment(),
    });
  } else if (operation === "down")
    await run("docker", [...compose, "down"], { env: await environment() });
  else if (operation === "inspect") {
    if (args.length)
      throw new Error("Use OPERATION_ID for migration inspection.");
    await run(
      "cargo",
      [
        "run",
        "--locked",
        "--offline",
        "-p",
        "darkhorse-server",
        "--",
        ...operatorArgs("migration-inspect", [process.env.OPERATION_ID]),
      ],
      { env: await environment() },
    );
  } else if (operation === "run")
    await run(
      "cargo",
      [
        "run",
        "--locked",
        "--offline",
        "-p",
        "darkhorse-server",
        "--",
        "--yes",
        ...args,
      ],
      { env: await environment() },
    );
  else
    throw new Error(
      "Use setup, up, down, or run with an explicit operator command.",
    );
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
