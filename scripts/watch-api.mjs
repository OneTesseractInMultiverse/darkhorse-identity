import { watch } from "node:fs";
import { startProcess, interruption } from "./lib/process.mjs";

const controller = interruption();
let child;
let timer;
let restarting = Promise.resolve();
let stopping = false;
const spec = {
  command: "cargo",
  args: ["run", "--locked", "--offline", "-p", "darkhorse-server"],
};

async function restart() {
  if (stopping) return;
  const previous = child;
  child = undefined;
  await previous?.stop();
  child = startProcess(spec);
  monitor(child);
}

function monitor(running) {
  void running.done.then(() => {
    if (child === running && !stopping) fail();
  }, fail);
}

function fail() {
  console.error(
    "Rust process stopped; fix the reported error and restart make dev.",
  );
  process.exitCode = 1;
  controller.abort();
}

function schedule(_event, filename) {
  if (!filename || !/\.(rs|toml)$/.test(filename)) return;
  clearTimeout(timer);
  timer = setTimeout(() => {
    restarting = restarting.then(restart).catch(fail);
  }, 200);
}

const watchers = [
  "crates",
  "apps/server",
  "Cargo.toml",
  "rust-toolchain.toml",
].map((path) => watch(path, { recursive: true }, schedule));
try {
  await restart();
  await new Promise((resolve) => {
    if (controller.signal.aborted) resolve();
    else controller.signal.addEventListener("abort", resolve, { once: true });
  });
} finally {
  stopping = true;
  clearTimeout(timer);
  watchers.forEach((watcher) => watcher.close());
  await restarting;
  await child?.stop();
}
