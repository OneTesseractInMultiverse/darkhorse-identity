import { randomBytes } from "node:crypto";
import { mkdir, lstat, readFile, writeFile } from "node:fs/promises";
import { run } from "./lib/command.mjs";
import { operatorArgs } from "./lib/deployment-plan.mjs";
async function main() {
  const [mode, ...args] = process.argv.slice(2);
  if (mode === "inspect" && !args.length) {
    const command = operatorArgs("signing-inspect", [process.env.OPERATION_ID]);
    await run(process.execPath, ["scripts/database.mjs", "run", ...command]);
    return;
  }
  const path = ".local/signing-wrap.key";
  if (mode === "setup") {
    process.umask(0o077);
    await mkdir(".local", { recursive: true, mode: 0o700 });
    try {
      await writeFile(path, randomBytes(32).toString("hex"), {
        flag: "wx",
        mode: 0o600,
      });
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
    }
    console.log("Local signing wrapping key is ready; existing key preserved.");
    return;
  }
  const info = await lstat(path);
  if (!info.isFile() || info.mode & 0o077)
    throw new Error("Private wrapping key file required.");
  const key = (await readFile(path, "utf8")).trim();
  if (!/^[a-f0-9]{64}$/.test(key)) throw new Error("Invalid wrapping key.");
  const env = {
    ...process.env,
    DARKHORSE_PROVIDER_ENABLED: "true",
    DARKHORSE_SIGNING_WRAP_KEY: key,
  };
  if (mode === "dev" && args.length === 0)
    await run(process.execPath, ["scripts/login.mjs", "dev"], { env });
  else if (mode === "run" && args[0]?.startsWith("signing-"))
    await run(process.execPath, ["scripts/database.mjs", "run", ...args], {
      env,
    });
  else
    throw new Error(
      "Use setup, dev, or run with an explicit signing operation.",
    );
}
main().catch(() => {
  console.error(
    "Provider operation failed; check setup, private file permissions, public origin and operator arguments.",
  );
  process.exitCode = 1;
});
