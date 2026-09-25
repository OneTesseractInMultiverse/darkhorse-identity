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
function invoke(mode, override = {}, script = "scripts/account.mjs", makeMode) {
  const group = script === "scripts/catalog.mjs" ? "catalog" : "account";
  const searchKey = override.CATALOG_NAME
    ? "CATALOG_NAME"
    : `${group.toUpperCase()}_SEARCH`;
  return new Promise((done, reject) => {
    const child = spawn(
      makeMode ? "make" : process.execPath,
      makeMode
        ? [
            "--no-print-directory",
            `kube-${group}-run`,
            ...(makeMode === "arguments"
              ? [`${searchKey}=${override[searchKey]}`]
              : []),
          ]
        : [script, "kube-run"],
      {
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
      },
    );
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
async function scenarios(script = "scripts/account.mjs") {
  const catalog = script === "scripts/catalog.mjs";
  const settings = catalog
    ? {
        ACCOUNT_OPERATION: "",
        ACCOUNT_ID: "",
        ACCOUNT_CONFIRM: "",
        CATALOG_TARGET: "client",
        CATALOG_APPLICATION_ID: "00000000-0000-0000-0000-000000000001",
        CATALOG_SEARCH: "--literal $(text)",
      }
    : {};
  const run = (mode, extra = {}) =>
    invoke(mode, { ...settings, ...extra }, script);
  const namedLikeCommand = await run("success", { KUBE_CONTEXT: "exec" });
  assert.equal(namedLikeCommand.code, 0, namedLikeCommand.stderr);
  if (catalog) {
    for (const target of ["application", "client"]) {
      await writeFile(events, "");
      const client = "00000000-0000-0000-0000-000000000002";
      const result = await invoke(
        "success",
        {
          ...settings,
          CATALOG_TARGET: target,
          CATALOG_OPERATION: "show",
          CATALOG_SEARCH: "",
          CATALOG_CLIENT_ID: target === "client" ? client : "",
        },
        script,
        "environment",
      );
      assert.equal(result.code, 0, result.stderr);
      const calls = (await readFile(events, "utf8"))
        .trim()
        .split("\n")
        .map(JSON.parse);
      const execution = calls.filter((v) => v.phase === "exec");
      assert.equal(execution.length, 1);
      const args = execution[0].args;
      assert.deepEqual(args.slice(args.indexOf("operator")), [
        "operator",
        target,
        "show",
        settings.CATALOG_APPLICATION_ID,
        ...(target === "client" ? [client] : []),
      ]);
      assert.equal(calls.filter((v) => v.phase === "delete").length, 1);
      assert.ok(
        !result.stdout.includes(marker) && !result.stderr.includes(marker),
      );
    }
  }
  if (catalog)
    for (const operation of ["create", "update"])
      for (const makeMode of ["environment", "arguments"]) {
        await writeFile(events, "");
        const name = "--$(shell printf EXPANDED) `literal` %_\\";
        const owner = "00000000-0000-0000-0000-000000000123";
        const result = await invoke(
          "success",
          {
            ...settings,
            CATALOG_TARGET: "application",
            CATALOG_OPERATION: operation,
            CATALOG_APPLICATION_ID: operation === "update" ? owner : "",
            CATALOG_REVISION: operation === "update" ? "0" : "",
            CATALOG_SEARCH: "",
            CATALOG_NAME: name,
            CATALOG_OWNER_ID: owner,
            CATALOG_STATUS: "inactive",
            CATALOG_CONFIRM: "yes",
          },
          script,
          makeMode,
        );
        assert.equal(result.code, 0, result.stderr);
        const calls = (await readFile(events, "utf8"))
          .trim()
          .split("\n")
          .map(JSON.parse);
        const args = calls.find((c) => c.phase === "exec").args;
        assert.ok(args.includes(`--name=${name}`));
        assert.ok(args.includes("--yes"));
        assert.deepEqual(args.slice(args.indexOf("operator")), [
          "operator",
          "application",
          operation,
          ...(operation === "update" ? [owner, "0"] : []),
          `--name=${name}`,
          "--owner",
          owner,
          "--status",
          "inactive",
        ]);
        assert.equal(calls.filter((c) => c.phase === "exec").length, 1);
        assert.ok(
          !result.stdout.includes(marker) && !result.stderr.includes(marker),
        );
      }
  if (catalog)
    for (const makeMode of ["environment", "arguments"]) {
      await writeFile(events, "");
      const client = "00000000-0000-0000-0000-000000000002";
      const result = await invoke(
        "success",
        {
          ...settings,
          CATALOG_OPERATION: "update",
          CATALOG_SEARCH: "",
          CATALOG_CLIENT_ID: client,
          CATALOG_REVISION: "0",
          CATALOG_CONFIRM: "yes",
        },
        script,
        makeMode,
      );
      assert.equal(result.code, 0, result.stderr);
      const calls = (await readFile(events, "utf8"))
        .trim()
        .split("\n")
        .map(JSON.parse);
      const execution = calls.filter((c) => c.phase === "exec");
      assert.equal(execution.length, 1);
      const args = execution[0].args;
      assert.deepEqual(args.slice(args.indexOf("operator")), [
        "operator",
        "client",
        "update",
        settings.CATALOG_APPLICATION_ID,
        client,
        "0",
      ]);
      assert.ok(args.includes("--auth-stdin") && args.includes("--yes"));
      assert.equal(calls.filter((c) => c.phase === "delete").length, 1);
      assert.ok(
        !result.stdout.includes(marker) && !result.stderr.includes(marker),
      );
    }
  if (catalog)
    for (const operation of ["list", "retire"])
      for (const makeMode of ["environment", "arguments"]) {
        await writeFile(events, "");
        const client = "00000000-0000-0000-0000-000000000002",
          secret = "00000000-0000-0000-0000-000000000003";
        const result = await invoke(
          "success",
          {
            ...settings,
            CATALOG_TARGET: "client-secret",
            CATALOG_OPERATION: operation,
            CATALOG_SEARCH: "",
            CATALOG_CLIENT_ID: client,
            ...(operation === "retire"
              ? {
                  CATALOG_SECRET_ID: secret,
                  CATALOG_REVISION: "0",
                  CATALOG_CONFIRM: "yes",
                }
              : { CATALOG_LIMIT: "2", CATALOG_AFTER: secret }),
          },
          script,
          makeMode,
        );
        assert.equal(result.code, 0, result.stderr);
        const calls = (await readFile(events, "utf8"))
          .trim()
          .split("\n")
          .map(JSON.parse);
        const execution = calls.filter((c) => c.phase === "exec");
        assert.equal(execution.length, 1);
        const args = execution[0].args;
        assert.deepEqual(args.slice(args.indexOf("operator")), [
          "operator",
          "client",
          "secret",
          operation,
          settings.CATALOG_APPLICATION_ID,
          client,
          ...(operation === "retire"
            ? [secret, "0"]
            : ["--limit", "2", "--after", secret]),
        ]);
        assert.equal(args.includes("--yes"), operation === "retire");
        assert.ok(args.includes("--auth-stdin"));
        assert.equal(calls.filter((c) => c.phase === "delete").length, 1);
        assert.ok(
          !result.stdout.includes(marker) && !result.stderr.includes(marker),
        );
      }
  for (const makeMode of ["environment", "arguments"]) {
    await writeFile(events, "");
    const search = "--$(shell printf EXPANDED) `literal` %_\\";
    const result = await invoke(
      "success",
      catalog
        ? { ...settings, CATALOG_SEARCH: search }
        : { ACCOUNT_OPERATION: "list", ACCOUNT_ID: "", ACCOUNT_SEARCH: search },
      script,
      makeMode,
    );
    assert.equal(result.code, 0, result.stderr);
    const calls = (await readFile(events, "utf8"))
      .trim()
      .split("\n")
      .map(JSON.parse);
    assert.ok(
      calls.find((v) => v.phase === "exec").args.includes(`--search=${search}`),
      "Make must preserve selector bytes without expanding embedded Make expressions",
    );
    assert.ok(
      !result.stdout.includes(marker) && !result.stderr.includes(marker),
    );
  }
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
    const result = await run(mode);
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
    if (executed) {
      const args = calls.find((v) => v.phase === "exec").args;
      assert.equal(args.includes("-t"), false);
      assert.ok(args.includes("--auth-stdin"));
      if (catalog) {
        assert.ok(args.includes("--search=--literal $(text)"));
        assert.deepEqual(
          args.slice(args.indexOf("operator"), args.indexOf("operator") + 4),
          ["operator", "client", "list", settings.CATALOG_APPLICATION_ID],
        );
      }
    }
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
  if (catalog)
    assert.equal(
      (
        await run("success", {
          CATALOG_TARGET: "application",
          CATALOG_APPLICATION_ID: "",
        })
      ).code,
      0,
    );
  for (const invalid of [
    { KUBE_ACCESS: "relative" },
    { KUBE_CONTEXT: "" },
    catalog ? { CATALOG_APPLICATION_ID: "bad" } : { ACCOUNT_ID: "bad" },
    { KUBE_CONFIG: directory },
  ]) {
    await writeFile(events, "");
    const result = await run("success", invalid);
    assert.equal(result.code, 2);
    assert.equal(await readFile(events, "utf8"), "");
    assert.ok(!result.stderr.includes(marker));
  }
}
try {
  await prepare();
  await scenarios();
  await scenarios("scripts/catalog.mjs");
  console.log(
    "One-shot Kubernetes account/catalog launcher process checks passed: stdin isolation, single execution, rejected configuration/creation/replacement, signals and conditional cleanup failures.",
  );
} finally {
  await rm(directory, { recursive: true, force: true });
}
