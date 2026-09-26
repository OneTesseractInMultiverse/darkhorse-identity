import { provisionBucket } from "./objects-provision.mjs";
import { randomBytes } from "node:crypto";
import { setTimeout as delay } from "node:timers/promises";
import { objectsImage } from "./boundary-images.mjs";
import { resourceLabels } from "./boundary-ci.mjs";
// Disposable loopback fixture. Production endpoints require TLS and scoped bucket credentials.
export async function objectService(command, docker) {
  const name = `darkhorse-objects-test-${randomBytes(8).toString("hex")}`;
  const access = randomBytes(16).toString("hex"),
    secret = randomBytes(32).toString("hex");
  const close = () =>
    docker(["rm", "--force", "--volumes", name], { acceptFailure: true });
  try {
    await docker(
      [
        "run",
        "--detach",
        ...resourceLabels(process.env.DARKHORSE_TEST_RUN_ID),
        "--name",
        name,
        "--publish",
        "127.0.0.1::9000",
        "--env",
        "RUSTFS_ACCESS_KEY",
        "--env",
        "RUSTFS_SECRET_KEY",
        "--env",
        "RUSTFS_CONSOLE_ENABLE=false",
        "--memory",
        "768m",
        "--cpus",
        "2",
        objectsImage,
      ],
      {
        env: {
          ...process.env,
          RUSTFS_ACCESS_KEY: access,
          RUSTFS_SECRET_KEY: secret,
        },
      },
    );
    const port = (await docker(["port", name, "9000/tcp"])).stdout
      .trim()
      .split(":")
      .at(-1);
    const endpoint = `http://127.0.0.1:${port}`;
    let ready = false;
    for (let i = 0; i < 120; i++) {
      try {
        const result = await fetch(`${endpoint}/health`, {
          signal: AbortSignal.timeout(500),
        });
        if (result.status < 500) {
          ready = true;
          break;
        }
      } catch {}
      await delay(250);
    }
    if (!ready) throw new Error("Disposable object storage did not start.");
    await provisionBucket(command, endpoint, "darkhorse-test", access, secret);
    return {
      close,
      settings: {
        DARKHORSE_OBJECTS_ENABLED: "true",
        DARKHORSE_OBJECTS_ENDPOINT: endpoint,
        DARKHORSE_OBJECTS_BUCKET: "darkhorse-test",
        DARKHORSE_OBJECTS_REGION: "us-east-1",
        DARKHORSE_OBJECTS_ACCESS_KEY: access,
        DARKHORSE_OBJECTS_SECRET_KEY: secret,
        DARKHORSE_OBJECTS_LOCAL_HTTP: "true",
      },
    };
  } catch (error) {
    await close();
    throw error;
  }
}
