import { parentPort, workerData } from "node:worker_threads";
import { DatabaseSync } from "node:sqlite";
import { createLogoutStore } from "../../lib/reference-logout-store.mjs";

const database = new DatabaseSync(workerData.path);
try {
  const store = createLogoutStore(database, () => 1000);
  parentPort.postMessage("ready");
  parentPort.once("message", () => {
    try {
      parentPort.postMessage(
        workerData.action === "logout"
          ? store.accept(workerData.claims, "same-token")
          : store.bind("racing-callback", workerData.claims),
      );
    } finally {
      database.close();
      parentPort.close();
    }
  });
} catch (error) {
  database.close();
  throw error;
}
