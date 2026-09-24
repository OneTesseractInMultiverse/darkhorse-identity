import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

process.chdir(resolve(import.meta.dirname, ".."));
const directory = await mkdtemp(join(tmpdir(), "darkhorse-account-test-"));
const runner = join(directory, "runner.mjs"),
  fixture = join(directory, "fixture.mjs");
const marker = "source-defined-protected-stdin-marker";
function invoke(file, args, input, env = process.env, interrupt = false) {
  return new Promise((resolveResult, reject) => {
    const child = spawn(process.execPath, [file, ...args], {
      env,
      stdio: "pipe",
    });
    let stdout = "",
      stderr = "";
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error("Launcher process test exceeded its deadline."));
    }, 10000);
    child.stdout.on("data", (bytes) => {
      stdout += bytes;
      if (interrupt && stdout.includes("ready")) {
        interrupt = false;
        child.kill("SIGTERM");
      }
    });
    child.stderr.on("data", (bytes) => {
      stderr += bytes;
    });
    child.once("error", reject);
    child.once("close", (code) => {
      clearTimeout(timer);
      resolveResult({ code, stdout, stderr });
    });
    child.stdin.on("error", () => {});
    child.stdin.end(input);
  });
}
async function prepare() {
  await writeFile(
    runner,
    `import { transport } from ${JSON.stringify(pathToFileURL(resolve("scripts/lib/account-transport.mjs")).href)};
const result = await transport({command:process.execPath,args:[${JSON.stringify(fixture)}, ...process.argv.slice(2)]}, process.argv[2] === "timeout" ? 1500 : 8000);
console.error(JSON.stringify(result)); process.exitCode = result.code;`,
    { mode: 0o600 },
  );
  await writeFile(
    fixture,
    `import { spawn } from "node:child_process";
import { writeFileSync, appendFileSync } from "node:fs";
const [mode, status] = process.argv.slice(2);
const base = ${JSON.stringify(directory)};
if (mode === "echo") {
  let input = ""; for await (const bytes of process.stdin) input += bytes;
  console.log(JSON.stringify({received:input === ${JSON.stringify(marker)}, tty:Boolean(process.stdin.isTTY), leaked:process.argv.join(" ").includes(input) || Object.values(process.env).some(value => value.includes(input))}));
  process.exitCode=Number(status);
} else {
  appendFileSync(base + "/starts", "1");
  process.on("SIGTERM", () => { writeFileSync(base + "/parent-stopped", "yes"); setTimeout(()=>process.exit(0),100); });
  spawn(process.execPath, ["-e", 'const fs=require("node:fs");process.on("SIGTERM",()=>{fs.writeFileSync('+JSON.stringify(base + "/descendant-stopped")+',"yes");process.exit(0)});console.log("ready");setInterval(()=>{},1000)'], {stdio:"inherit"});
  setInterval(()=>{},1000);
}`,
    { mode: 0o600 },
  );
}
async function streams() {
  for (const code of [0, 1, 2, 3, 74, 125]) {
    const result = await invoke(runner, ["echo", String(code)], marker);
    assert.equal(result.code, code);
    assert.deepEqual(JSON.parse(result.stdout), {
      received: true,
      tty: false,
      leaked: false,
    });
    assert.deepEqual(JSON.parse(result.stderr), { code, uncertain: false });
    assert.ok(
      !result.stdout.includes(marker) && !result.stderr.includes(marker),
    );
  }
}
async function interruption() {
  const canary = spawn(process.execPath, ["-e", "setInterval(()=>{},1000)"], {
    stdio: "ignore",
  });
  const done = new Promise((resolveDone) => canary.once("close", resolveDone));
  try {
    for (const mode of ["timeout", "signal"]) {
      const result = await invoke(
        runner,
        [mode],
        "",
        process.env,
        mode === "signal",
      );
      const code = mode === "timeout" ? 124 : 143;
      assert.equal(result.code, code);
      assert.deepEqual(JSON.parse(result.stderr), { code, uncertain: true });
      for (const file of ["parent-stopped", "descendant-stopped"]) {
        assert.equal(await readFile(join(directory, file), "utf8"), "yes");
        await rm(join(directory, file));
      }
      assert.equal(canary.exitCode, null);
      process.kill(canary.pid, 0);
    }
    assert.equal(
      await readFile(join(directory, "starts"), "utf8"),
      "11",
      "one launch per attempt, no retries",
    );
  } finally {
    canary.kill("SIGTERM");
    await done;
  }
}
async function rejectedSettings() {
  for (const extra of [
    { ACCOUNT_ID: marker },
    { ACCOUNT_OPERATION: marker },
    { ACCOUNT_CONFIRM: marker },
  ]) {
    const result = await invoke(
      "scripts/account.mjs",
      ["compose-exec"],
      marker,
      {
        ...process.env,
        STACK: "nonexistent-account-test",
        ACCOUNT_OPERATION: "show",
        ACCOUNT_ID: "00000000-0000-0000-0000-000000000001",
        ACCOUNT_REVISION: "",
        ACCOUNT_CONFIRM: "no",
        ...extra,
      },
    );
    assert.equal(result.code, 2);
    assert.equal(result.stdout, "");
    assert.ok(!result.stderr.includes(marker));
  }
}
async function rejectedConfiguration() {
  const settings = {
    ...process.env,
    ACCOUNT_OPERATION: "show",
    ACCOUNT_ID: "00000000-0000-0000-0000-000000000001",
    ACCOUNT_REVISION: "",
    ACCOUNT_CONFIRM: "no",
  };
  for (const file of [
    directory,
    join(directory, "missing.json"),
    join(directory, "invalid.json"),
  ]) {
    await writeFile(join(directory, "invalid.json"), "invalid " + marker);
    const result = await invoke("scripts/account.mjs", ["kube-exec"], marker, {
      ...settings,
      KUBE_CONFIG: file,
    });
    assert.ok([1, 2].includes(result.code));
    assert.equal(result.stdout, "");
    assert.ok(!result.stderr.includes(marker));
  }
  for (const args of [["compose-exec", marker], [marker]]) {
    const result = await invoke("scripts/account.mjs", args, marker, settings);
    assert.equal(result.code, 2);
    assert.equal(result.stdout, "");
    assert.ok(!result.stderr.includes(marker));
  }
}

async function catalogFailures() {
  for (const mode of ["compose-exec", "compose-run", "kube-exec", "kube-run"]) {
    for (const extra of [
      { CATALOG_TARGET: marker },
      { CATALOG_TARGET: "client", CATALOG_APPLICATION_ID: marker },
      { CATALOG_OPERATION: marker },
      { CATALOG_LIMIT: marker },
      { ACCOUNT_ID: marker },
    ]) {
      const result = await invoke("scripts/catalog.mjs", [mode], marker, {
        ...process.env,
        CATALOG_TARGET: "application",
        ...extra,
      });
      assert.equal(result.code, 2);
      assert.equal(result.stdout, "");
      assert.ok(!result.stderr.includes(marker));
    }
  }
  for (const args of [
    ["compose-exec", marker],
    [marker],
    ["kube-exec"],
    ["kube-run"],
  ]) {
    const result = await invoke("scripts/catalog.mjs", args, marker, {
      ...process.env,
      CATALOG_TARGET: "application",
      KUBE_CONFIG: join(directory, "invalid.json"),
    });
    assert.ok([1, 2].includes(result.code));
    assert.equal(result.stdout, "");
    assert.ok(!result.stderr.includes(marker));
  }
}

try {
  await prepare();
  await streams();
  await interruption();
  await rejectedSettings();
  await rejectedConfiguration();
  await catalogFailures();
  console.log(
    "Account/catalog launcher process checks passed: protected stdin, exit status, timeout/signals, owned descendant cleanup, unrelated process survival and no retry.",
  );
} finally {
  await rm(directory, { recursive: true, force: true });
}
