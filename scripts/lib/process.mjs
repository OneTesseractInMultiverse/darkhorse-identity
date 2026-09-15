import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";

export function startProcess({ command, args, env = process.env }) {
  const child = spawn(command, args, { env, stdio: "inherit", detached: true });
  const done = new Promise((resolve, reject) => {
    child.once("error", () =>
      reject(new Error(`Cannot start ${command}; install it or check PATH.`)),
    );
    child.once("exit", (code, signal) => resolve({ code, signal }));
  });
  // Attach a handler immediately; supervisors still receive the original rejection.
  void done.catch(() => {});
  return { done, stop: () => stopProcess(child, done) };
}

async function stopProcess(child, done) {
  if (!child.pid) return;
  signalGroup(child.pid, "SIGTERM");
  await Promise.race([done.catch(() => {}), delay(2000)]);
  signalGroup(child.pid, "SIGKILL");
  await done.catch(() => {});
}

function signalGroup(pid, signal) {
  try {
    process.kill(-pid, signal);
  } catch (error) {
    if (error.code !== "ESRCH") throw error;
  }
}

export function interruption() {
  const controller = new AbortController();
  process.once("SIGINT", () => controller.abort());
  process.once("SIGTERM", () => controller.abort());
  return controller;
}
