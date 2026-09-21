import { request } from "node:https";
export function httpsCall(
  origin,
  ca,
  path = "/",
  { method = "GET", headers = {}, body, servername } = {},
) {
  const url = new URL(path, origin);
  if (url.origin !== origin) throw new Error("Cross-origin probe rejected.");
  return new Promise((resolve, reject) => {
    const req = request(
      url,
      {
        ca,
        servername: servername ?? url.hostname,
        lookup: (_host, _options, done) =>
          done(null, [{ address: "127.0.0.1", family: 4 }]),
        method,
        headers,
        timeout: 15000,
      },
      (res) => {
        const chunks = [];
        let length = 0;
        res.on("data", (chunk) => {
          length += chunk.length;
          if (length > 2 * 1024 * 1024)
            req.destroy(new Error("Probe response too large."));
          else chunks.push(chunk);
        });
        res.on("error", () => reject(new Error("HTTPS response unavailable.")));
        res.on("end", () =>
          resolve({
            status: res.statusCode,
            headers: res.headers,
            text: Buffer.concat(chunks).toString("utf8"),
          }),
        );
      },
    );
    req.on("error", () => reject(new Error("Verified HTTPS probe failed.")));
    req.on("timeout", () => req.destroy(new Error("HTTPS deadline exceeded.")));
    req.end(body);
  });
}
