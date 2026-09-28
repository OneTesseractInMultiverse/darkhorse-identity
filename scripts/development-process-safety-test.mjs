import assert from "node:assert/strict";
import { createConnection, createServer } from "node:net";
import { requireFreePort } from "./lib/development-ports.mjs";
import { startProcess } from "./lib/process.mjs";

async function unusedPort() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const { port } = server.address();
  await new Promise((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
  return port;
}

function launched(command, args, marker) {
  let output = "";
  let resolveReady;
  let rejectReady;
  const ready = new Promise((resolve, reject) => {
    resolveReady = resolve;
    rejectReady = reject;
  });
  const child = startProcess({
    command,
    args,
    stdout: (chunk) => {
      output += chunk.toString();
      if (output.includes(marker)) resolveReady(output);
    },
  });
  const timeout = setTimeout(
    () =>
      rejectReady(new Error("Child process did not reach its ready state.")),
    5000,
  );
  child.done.then(
    (result) =>
      rejectReady(
        new Error(`Child process exited before readiness (${result.code}).`),
      ),
    rejectReady,
  );
  return {
    child,
    output: () => output,
    ready: ready.finally(() => clearTimeout(timeout)),
  };
}

async function probe(port) {
  const socket = createConnection({ host: "127.0.0.1", port });
  let response = "";
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(
      () => socket.destroy(new Error("Local canary did not respond.")),
      1000,
    );
    socket.on("data", (chunk) => (response += chunk.toString()));
    socket.once("end", () => {
      clearTimeout(timeout);
      resolve(response);
    });
    socket.once("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
  });
}

async function verifyOccupiedPortIsPreserved() {
  const port = await unusedPort();
  const canary = launched(
    process.execPath,
    [
      "-e",
      `const net=require("node:net");const server=net.createServer((socket)=>socket.end("canary\\n"));server.listen(Number(process.argv[1]),"127.0.0.1",()=>console.log("canary-ready"));process.on("SIGTERM",()=>server.close(()=>process.exit(0)));setInterval(()=>{},1000);`,
      String(port),
    ],
    "canary-ready",
  );
  try {
    await canary.ready;
    await assert.rejects(
      requireFreePort(port),
      new RegExp(`Port ${port} is occupied`),
    );
    assert.equal(await probe(port), "canary\n");
  } finally {
    await canary.child.stop();
  }
  await requireFreePort(port);
}

async function verifyOwnedProcessGroupCleanupPreservesOtherListeners() {
  const port = await unusedPort();
  const canary = launched(
    process.execPath,
    [
      "-e",
      `const net=require("node:net");const server=net.createServer((socket)=>socket.end("canary\\n"));server.listen(Number(process.argv[1]),"127.0.0.1",()=>console.log("canary-ready"));process.on("SIGTERM",()=>server.close(()=>process.exit(0)));setInterval(()=>{},1000);`,
      String(port),
    ],
    "canary-ready",
  );
  let owned;
  try {
    await canary.ready;
    owned = launched(
      process.execPath,
      [
        "-e",
        `const {spawn}=require("node:child_process");const child=spawn(process.execPath,["-e",'process.on("SIGTERM",()=>process.exit(0));setInterval(()=>{},1000)'],{stdio:"ignore"});process.on("SIGTERM",()=>{});child.on("exit",()=>{console.log("descendant-exited");process.exit(0)});console.log("descendant:"+child.pid);setInterval(()=>{},1000);`,
      ],
      "descendant:",
    );
    await owned.ready;
    await owned.child.stop();
    assert.match(owned.output(), /descendant-exited/);
    assert.equal(await probe(port), "canary\n");
  } finally {
    if (owned) await owned.child.stop();
    await canary.child.stop();
  }
}

await verifyOccupiedPortIsPreserved();
await verifyOwnedProcessGroupCleanupPreservesOtherListeners();
console.log(
  "Development process safety passed: occupied listeners remain untouched and cleanup stops only the owned process group.",
);
