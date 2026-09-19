import { createServer } from "node:tls";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { randomBytes } from "node:crypto";

// Disposable implicit-TLS SMTP receiver. Messages stay in memory and never enter logs.
export async function verificationMailbox(directory) {
  const username = randomBytes(16).toString("hex");
  const password = randomBytes(32).toString("hex");
  const messages = [],
    sockets = new Set();
  const key = await readFile(join(directory, "server.key"));
  const cert = await readFile(join(directory, "server.pem"));
  const server = createServer(
    { key, cert, minVersion: "TLSv1.2" },
    (socket) => {
      sockets.add(socket);
      socket.on("close", () => sockets.delete(socket));
      socket.on("error", () => socket.destroy());
      socket.setTimeout(15_000, () => socket.destroy());
      let buffer = "",
        data = false,
        body = "",
        authenticated = false;
      socket.write("220 localhost test SMTP\r\n");
      socket.on("data", (bytes) => {
        buffer += bytes.toString("utf8");
        if (buffer.length + body.length > 16_384) return socket.destroy();
        let end;
        while ((end = buffer.indexOf("\r\n")) !== -1) {
          const line = buffer.slice(0, end);
          buffer = buffer.slice(end + 2);
          if (data) {
            if (line === ".") {
              messages.push(body);
              body = "";
              data = false;
              socket.write("250 queued\r\n");
            } else body += `${line}\r\n`;
          } else if (line.startsWith("EHLO "))
            socket.write("250-localhost\r\n250 AUTH PLAIN\r\n");
          else if (line.startsWith("AUTH PLAIN ")) {
            authenticated =
              Buffer.from(line.slice(11), "base64").toString() ===
              `\0${username}\0${password}`;
            socket.write(
              authenticated ? "235 authenticated\r\n" : "535 denied\r\n",
            );
          } else if (!authenticated)
            socket.write("530 authentication required\r\n");
          else if (line.startsWith("MAIL FROM:") || line.startsWith("RCPT TO:"))
            socket.write("250 ok\r\n");
          else if (line === "DATA") {
            data = true;
            socket.write("354 send message\r\n");
          } else if (line === "QUIT") {
            socket.end("221 goodbye\r\n");
          } else socket.write("500 unsupported\r\n");
        }
      });
    },
  );
  server.on("tlsClientError", () => {});
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  return {
    settings: {
      DARKHORSE_EMAIL_ENABLED: "true",
      DARKHORSE_EMAIL_KEY: randomBytes(32).toString("hex"),
      DARKHORSE_SMTP_HOST: "localhost",
      DARKHORSE_SMTP_PORT: String(server.address().port),
      DARKHORSE_SMTP_FROM: "identity@example.com",
      DARKHORSE_SMTP_USERNAME: username,
      DARKHORSE_SMTP_PASSWORD: password,
      DARKHORSE_SMTP_CA_FILE: join(directory, "ca.pem"),
    },
    messages,
    async close() {
      for (const socket of sockets) socket.destroy();
      await new Promise((resolve) => server.close(resolve));
      messages.length = 0;
    },
  };
}
