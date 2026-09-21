import assert from "node:assert/strict";
import { mkdir, mkdtemp, writeFile, chmod, rm } from "node:fs/promises";
import { resolve, join } from "node:path";
export async function verifySecretFiles(executable, environment, command) {
  await mkdir(".local", { recursive: true, mode: 0o700 });
  const directory = await mkdtemp(resolve(".local/secret-file-test-"));
  const path = join(directory, "database");
  const env = { ...environment, DARKHORSE_DATABASE_URL_FILE: path };
  delete env.DARKHORSE_DATABASE_URL;
  const invoke = (values) =>
    command(executable, ["migrate"], {
      env: values,
      input: "",
      capture: true,
      acceptFailure: true,
    });
  try {
    await writeFile(path, environment.DARKHORSE_DATABASE_URL + "\r\n", {
      mode: 0o600,
    });
    assert.equal((await invoke(env)).code, 0);
    const conflict = await invoke({
      ...env,
      DARKHORSE_DATABASE_URL: environment.DARKHORSE_DATABASE_URL,
    });
    assert.notEqual(conflict.code, 0);
    for (const bytes of [
      Buffer.alloc(0),
      Buffer.from([255]),
      Buffer.alloc(32769, 120),
      Buffer.from("bad\0secret"),
    ]) {
      await writeFile(path, bytes);
      assert.notEqual((await invoke(env)).code, 0);
    }
    await writeFile(path, environment.DARKHORSE_DATABASE_URL);
    await chmod(path, 0o666);
    assert.notEqual((await invoke(env)).code, 0);
    await chmod(path, 0o600);
    for (const file of [
      directory,
      join(directory, "missing"),
      "relative-secret",
      "/bad\npath",
    ]) {
      const failed = await invoke({
        ...env,
        DARKHORSE_DATABASE_URL_FILE: file,
      });
      assert.notEqual(failed.code, 0);
      assert.ok(!failed.stderr.includes(environment.DARKHORSE_DATABASE_URL));
      assert.ok(!failed.stderr.includes(file));
    }
    console.log(
      "Real configuration files: successful binding, conflict, bounds, encoding, permissions, missing/nonregular paths and redacted failures passed.",
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}
