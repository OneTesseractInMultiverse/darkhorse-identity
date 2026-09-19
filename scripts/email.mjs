import { randomBytes } from "node:crypto";
import { mkdir, lstat, readFile, writeFile } from "node:fs/promises";
import { run } from "./lib/command.mjs";
async function main() {
  const [mode, ...args] = process.argv.slice(2);
  const path = ".local/email-verification.key";
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
    console.log(
      "Local email verification key is ready; existing key preserved.",
    );
    return;
  }
  const info = await lstat(path);
  if (!info.isFile() || info.mode & 0o077)
    throw new Error("Private email key file required.");
  const key = (await readFile(path, "utf8")).trim();
  if (!/^[a-f0-9]{64}$/.test(key)) throw new Error("Invalid email key.");
  const env = {
    ...process.env,
    DARKHORSE_EMAIL_ENABLED: "true",
    DARKHORSE_EMAIL_KEY: key,
  };
  if (mode === "dev" && args.length === 0)
    await run(process.execPath, ["scripts/login.mjs", "dev"], { env });
  else throw new Error("Use setup or dev with configured SMTP delivery.");
}
main().catch(() => {
  console.error(
    "Email operation failed; check login setup, private key permissions and SMTP configuration.",
  );
  process.exitCode = 1;
});
