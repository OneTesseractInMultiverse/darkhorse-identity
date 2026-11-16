// Shared test-only listener bounds for the standalone and deployed-route fixtures.
import { createServer } from "node:https";

export async function readLogoutBody(request) {
  const chunks = [];
  let bytes = 0;
  for await (const chunk of request) {
    bytes += chunk.length;
    if (bytes > 16384) throw new Error("Body limit.");
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}
export function createLogoutServer(tls, receive) {
  const server = createServer({ ...tls, maxHeaderSize: 8192 }, receive);
  server.requestTimeout = 5000;
  server.headersTimeout = 5000;
  server.timeout = 5000;
  server.maxConnections = 32;
  return server;
}
