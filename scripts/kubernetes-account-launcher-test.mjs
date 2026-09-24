import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
process.chdir(resolve(import.meta.dirname, ".."));
const directory = await mkdtemp(join(tmpdir(), "darkhorse-kube-account-"));
const marker = "source-defined-private-account-input";
const config = join(directory, "configuration.json");
const events = join(directory, "events");
async function prepare() {
  await writeFile(
    config,
    JSON.stringify({
      namespace: "identity-app",
      backendNamespace: "identity-state",
      ingressNamespace: "identity-ingress",
      origin: "https://identity.example.com",
      image: "registry.example.com/darkhorse@sha256:" + "a".repeat(64),
      edgeImage: "registry.example.com/edge@sha256:" + "b".repeat(64),
    }),
  );
  await writeFile(
    join(directory, "kubectl"),
    `#!${process.execPath}
const fs = require("node:fs");
const path = require("node:path");
const base = ${JSON.stringify(directory)};
const mode = process.env.FIXTURE_MODE;
const args = process.argv.slice(2);
const phase = args[7];
fs.appendFileSync(path.join(base,"events"), JSON.stringify({phase,args}) + "\\n");
const uid = "00000000-0000-0000-0000-000000000123";
async function input() { let text=""; for await (const bytes of process.stdin) text += bytes; return text; }
(async()=>{
if (phase === "create") {
  const p = JSON.parse(await input());
  if (["output-limit", "setup-signal"].includes(mode)) {
    process.on("SIGTERM",()=>{});
    fs.writeFileSync(path.join(base,"control-pid"),String(process.pid));
    if(mode === "output-limit") process.stdout.write("x".repeat(1048577));
    setInterval(()=>{},1000); return;
  }
  if (mode === "create-fail") { console.error(${JSON.stringify(marker)}); process.exitCode=1; return; }
  p.metadata.uid = uid;
  p.status = {phase:"Running",containerStatuses:[{name:"api",ready:true,restartCount:0,state:{running:{}}}]};
  fs.writeFileSync(path.join(base,"pod"),JSON.stringify(p));
  console.log(mode === "malformed" ? ${JSON.stringify(marker)} : JSON.stringify(p));
} else if (phase === "get") {
  const p=JSON.parse(fs.readFileSync(path.join(base,"pod")));
  if(mode === "replacement") p.metadata.uid="00000000-0000-0000-0000-000000000456";
  console.log(JSON.stringify(p));
} else if (phase === "wait") {
  if(mode === "wait-fail" && args.includes("--for=condition=Ready")) process.exitCode=1;
} else if (phase === "exec") {
  const received = await input();
  if(received !== ${JSON.stringify(marker)} || args.some(v=>v.includes(received)) || Object.values(process.env).some(v=>v.includes(received))) process.exit(77);
  if(mode === "signal") { console.log("ready"); setInterval(()=>{},1000); }
  else { console.log(JSON.stringify({received:true,tty:Boolean(process.stdin.isTTY)})); process.exitCode=mode === "denied" ? 3 : 0; }
} else if (phase === "delete") {
  const body=JSON.parse(await input());
  if(body.preconditions.uid !== uid || body.gracePeriodSeconds !== 5 || !args.includes("--raw")) process.exit(78);
  if(mode === "cleanup-fail") process.exitCode=1;
} else process.exit(79);
})();`,
    { mode: 0o700 },
  );
}
function invoke(mode, override = {}) {
  return new Promise((done, reject) => {
    const child = spawn(process.execPath, ["scripts/account.mjs", "kube-run"], {
      env: {
        ...process.env,
        PATH: directory + ":" + process.env.PATH,
        KUBE_CONFIG: config,
        KUBE_ACCESS: join(directory, "access"),
        KUBE_CONTEXT: "fixture",
        ACCOUNT_ID: "00000000-0000-0000-0000-000000000001",
        ACCOUNT_OPERATION: "show",
        ACCOUNT_REVISION: "",
        ACCOUNT_CONFIRM: "no",
        FIXTURE_MODE: mode,
        ...override,
      },
      stdio: "pipe",
    });
    let stdout = "",
      stderr = "",
      sent = false;
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error("Kubernetes launcher fixture exceeded deadline."));
    }, 10000);
    const monitor =
      mode === "setup-signal"
        ? setInterval(async () => {
            try {
              await readFile(join(directory, "control-pid"));
              if (!sent) {
                sent = true;
                child.kill("SIGTERM");
              }
            } catch {}
          }, 10)
        : undefined;
    child.stdout.on("data", (bytes) => {
      stdout += bytes;
      if (mode === "signal" && stdout.includes("ready") && !sent) {
        sent = true;
        child.kill("SIGTERM");
      }
    });
    child.stderr.on("data", (bytes) => {
      stderr += bytes;
    });
    child.once("error", reject);
    child.once("close", (code) => {
      clearTimeout(timer);
      clearInterval(monitor);
      done({ code, stdout, stderr });
    });
    child.stdin.on("error", () => {});
    child.stdin.end(marker);
  });
}
async function scenarios() {
  const namedLikeCommand = await invoke("success", { KUBE_CONTEXT: "exec" });
  assert.equal(namedLikeCommand.code, 0, namedLikeCommand.stderr);
  for (const [mode, code, executed, removed] of [
    ["success", 0, 1, 1],
    ["denied", 3, 1, 1],
    ["signal", 143, 1, 1],
    ["setup-signal", 143, 0, 0],
    ["output-limit", 1, 0, 0],
    ["create-fail", 1, 0, 0],
    ["malformed", 1, 0, 0],
    ["replacement", 1, 0, 1],
    ["wait-fail", 1, 0, 1],
    ["cleanup-fail", 1, 1, 1],
  ]) {
    await writeFile(events, "");
    const result = await invoke(mode);
    assert.equal(result.code, code, `${mode}: ${result.stderr}`);
    assert.ok(
      !result.stdout.includes(marker) && !result.stderr.includes(marker),
    );
    const recorded = await readFile(events, "utf8");
    assert.ok(!recorded.includes(marker));
    const calls = recorded.trim().split("\n").map(JSON.parse);
    assert.equal(calls.filter((v) => v.phase === "create").length, 1);
    assert.equal(calls.filter((v) => v.phase === "exec").length, executed);
    assert.equal(calls.filter((v) => v.phase === "delete").length, removed);
    if (mode === "success" || mode === "denied")
      assert.deepEqual(JSON.parse(result.stdout), {
        received: true,
        tty: false,
      });
    if (mode === "cleanup-fail")
      assert.match(result.stderr, /cleanup could not be confirmed/);
    if (mode === "signal")
      assert.match(result.stderr, /may continue or have committed/);
    if (["output-limit", "setup-signal"].includes(mode)) {
      const pid = Number(
        await readFile(join(directory, "control-pid"), "utf8"),
      );
      assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
      await rm(join(directory, "control-pid"));
      assert.equal(result.stdout, "");
    }
  }
  for (const settings of [
    { KUBE_ACCESS: "relative" },
    { KUBE_CONTEXT: "" },
    { ACCOUNT_ID: "bad" },
    { KUBE_CONFIG: directory },
  ]) {
    await writeFile(events, "");
    const result = await invoke("success", settings);
    assert.equal(result.code, 2);
    assert.equal(await readFile(events, "utf8"), "");
    assert.ok(!result.stderr.includes(marker));
  }
}
try {
  await prepare();
  await scenarios();
  console.log(
    "One-shot Kubernetes launcher process checks passed: stdin isolation, single execution, rejected configuration/creation/replacement, signals and conditional cleanup failures.",
  );
} finally {
  await rm(directory, { recursive: true, force: true });
}
