import { createServer, connect } from "node:net";
import { createServer as createTlsServer } from "node:tls";
import { readFile, writeFile, chmod } from "node:fs/promises";
import { join } from "node:path";
import { frame } from "./resp-frame.mjs";

async function listen(server, sockets) {
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  return {
    port: server.address().port,
    close: async () => {
      for (const socket of sockets) socket.destroy();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}
function track(socket, sockets) {
  sockets.add(socket);
  socket.on("close", () => sockets.delete(socket));
}
export async function lostReplyProxy(port, disconnect = true) {
  const sockets = new Set();
  let dropped = false;
  const server = createServer((client) => {
    const upstream = connect({ host: "127.0.0.1", port });
    track(client, sockets);
    track(upstream, sockets);
    const close = () => {
      client.destroy();
      upstream.destroy();
    };
    client.on("error", close);
    upstream.on("error", close);
    client.on("close", close);
    upstream.on("close", close);
    const pending = [];
    let requests = Buffer.alloc(0),
      responses = Buffer.alloc(0);
    client.on("data", (bytes) => {
      try {
        requests = Buffer.concat([requests, bytes]);
        for (let next; (next = frame(requests));) {
          pending.push(
            Array.isArray(next.value) &&
              next.value[0] === "EVALSHA" &&
              next.value[4] === "apply",
          );
          if (pending.length > 32)
            throw new Error("Proxy queue bound exceeded.");
          upstream.write(requests.subarray(0, next.end));
          requests = requests.subarray(next.end);
        }
      } catch {
        close();
      }
    });
    upstream.on("data", (bytes) => {
      try {
        responses = Buffer.concat([responses, bytes]);
        for (let next; (next = frame(responses));) {
          const charge = pending.shift();
          if (charge && next.value === "1" && !dropped) {
            dropped = true;
            responses = responses.subarray(next.end);
            if (disconnect) close();
            return;
          }
          client.write(responses.subarray(0, next.end));
          responses = responses.subarray(next.end);
        }
      } catch {
        close();
      }
    });
  });
  return listen(server, sockets);
}
export async function tlsProxy(port, directory, command) {
  const ca = join(directory, "ca.pem"),
    caKey = join(directory, "ca.key"),
    key = join(directory, "server.key"),
    csr = join(directory, "server.csr"),
    cert = join(directory, "server.pem"),
    extensions = join(directory, "server.ext");
  await command(
    "openssl",
    [
      "req",
      "-x509",
      "-newkey",
      "rsa:2048",
      "-nodes",
      "-keyout",
      caKey,
      "-out",
      ca,
      "-days",
      "1",
      "-subj",
      "/CN=Disposable Redis Test CA",
    ],
    { capture: true },
  );
  await command(
    "openssl",
    [
      "req",
      "-new",
      "-newkey",
      "rsa:2048",
      "-nodes",
      "-keyout",
      key,
      "-out",
      csr,
      "-subj",
      "/CN=localhost",
    ],
    { capture: true },
  );
  await writeFile(
    extensions,
    "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost\n",
    { mode: 0o600 },
  );
  await command(
    "openssl",
    [
      "x509",
      "-req",
      "-in",
      csr,
      "-CA",
      ca,
      "-CAkey",
      caKey,
      "-CAserial",
      join(directory, "ca.srl"),
      "-CAcreateserial",
      "-out",
      cert,
      "-days",
      "1",
      "-extfile",
      extensions,
    ],
    { capture: true },
  );
  await chmod(caKey, 0o600);
  await chmod(key, 0o600);
  const sockets = new Set();
  const server = createTlsServer(
    { key: await readFile(key), cert: await readFile(cert) },
    (client) => {
      const upstream = connect({ host: "127.0.0.1", port });
      track(client, sockets);
      track(upstream, sockets);
      const close = () => {
        client.destroy();
        upstream.destroy();
      };
      client.on("error", close);
      upstream.on("error", close);
      client.on("close", close);
      upstream.on("close", close);
      client.pipe(upstream).pipe(client);
    },
  );
  server.on("connection", (socket) => track(socket, sockets));
  return { ...(await listen(server, sockets)), ca: await readFile(ca, "utf8") };
}
