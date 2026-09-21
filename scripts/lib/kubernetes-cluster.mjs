import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { setTimeout as delay } from "node:timers/promises";
export const nodeImage =
  "kindest/node:v1.36.4@sha256:099e049362a1526b2db71494e1947aae99bd16290d7c895f2b7ea312e3cbfaed";
export async function localPort() {
  const server = createServer();
  await new Promise((r, reject) =>
    server.once("error", reject).listen(0, "127.0.0.1", r),
  );
  const p = server.address().port;
  await new Promise((r) => server.close(r));
  return p;
}
export async function loadImages(name, kind, command, images) {
  await command(kind, ["load", "docker-image", "--name", name, ...images], {
    capture: true,
  });
  const table = (
    await command(
      "docker",
      [
        "exec",
        `${name}-control-plane`,
        "ctr",
        "--namespace",
        "k8s.io",
        "images",
        "list",
      ],
      { capture: true },
    )
  ).stdout;
  const refs = [];
  for (const tag of images.slice(0, 2)) {
    const first = tag.split("/")[0];
    const source = !tag.includes("/")
      ? `docker.io/library/${tag}`
      : /[.:]/.test(first) || first === "localhost"
        ? tag
        : `docker.io/${tag}`;
    const line = table.split("\n").find((l) => l.startsWith(source + " "));
    if (!line) throw new Error("Loaded image not found in isolated node.");
    const digest = line.split(/\s+/)[2];
    if (!/^sha256:[a-f0-9]{64}$/.test(digest))
      throw new Error("Invalid loaded image digest.");
    const repository = source.includes("@")
      ? source.split("@")[0]
      : source.lastIndexOf(":") > source.lastIndexOf("/")
        ? source.slice(0, source.lastIndexOf(":"))
        : source;
    const ref = `${repository}@${digest}`;
    await command(
      "docker",
      [
        "exec",
        `${name}-control-plane`,
        "ctr",
        "--namespace",
        "k8s.io",
        "images",
        "tag",
        source,
        ref,
      ],
      { capture: true },
    );
    refs.push(ref);
  }
  return refs;
}
export async function forward(access, context, namespace, pod, children) {
  const port = await localPort();
  const child = spawn(
    "kubectl",
    [
      "--kubeconfig",
      access,
      "--context",
      context,
      "-n",
      namespace,
      "port-forward",
      "--address",
      "127.0.0.1",
      `pod/${pod}`,
      `${port}:8443`,
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  children.push(child);
  let ready = false,
    failed = false;
  child.once("error", () => {
    failed = true;
  });
  child.stdout.on("data", (data) => {
    if (data.toString().includes("Forwarding from")) ready = true;
  });
  child.stderr.on("data", () => {});
  for (let n = 0; n < 60; n++) {
    if (ready) return port;
    if (failed || child.exitCode !== null) break;
    await delay(100);
  }
  throw new Error("Owned pod port-forward did not become ready.");
}
