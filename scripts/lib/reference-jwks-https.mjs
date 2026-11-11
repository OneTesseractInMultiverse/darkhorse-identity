// Test-only HTTPS coordinator. No proxy, redirects, retries or shared agent.
import { request } from "node:https";
import { lookup } from "node:dns";
import { JWKS_LIMITS, validJwksResponse } from "./reference-jwks-policy.mjs";

export function createJwksTransport({
  send = request,
  resolveHost = lookup,
  schedule = setTimeout,
  cancel = clearTimeout,
} = {}) {
  let busy = false;
  return ({ url, ca }) => {
    if (busy) return Promise.reject(new Error("JWKS transport busy."));
    busy = true;
    return new Promise((resolve, reject) => {
      let req,
        closed = false,
        lookups = 0,
        settled = false;
      const release = () => {
        if (closed && lookups === 0) busy = false;
      };
      const finish = (error, body) => {
        if (settled) return;
        settled = true;
        cancel(deadline);
        if (error) {
          req?.destroy();
          reject(new Error("Trusted JWKS retrieval failed."));
        } else resolve(body);
      };
      const deadline = schedule(() => finish(true), JWKS_LIMITS.timeoutMs);
      try {
        req = send(
          url,
          {
            method: "GET",
            ca,
            rejectUnauthorized: true,
            agent: false,
            maxHeaderSize: JWKS_LIMITS.headerBytes,
            headers: {
              accept: "application/jwk-set+json, application/json",
              "accept-encoding": "identity",
            },
            lookup(hostname, options, callback) {
              lookups++;
              resolveHost(hostname, options, (...args) => {
                lookups--;
                callback(...args);
                release();
              });
            },
          },
          (response) => {
            if (
              !validJwksResponse(
                response.statusCode,
                response.headers,
                response.rawHeaders.length / 2,
              )
            ) {
              finish(true);
              response.destroy();
              return;
            }
            const chunks = [];
            let bytes = 0;
            response.on("data", (chunk) => {
              bytes += chunk.length;
              if (bytes > JWKS_LIMITS.responseBytes) {
                finish(true);
                response.destroy();
              } else chunks.push(chunk);
            });
            response.on("error", () => finish(true));
            response.on("end", () => finish(false, Buffer.concat(chunks)));
          },
        );
        // A timed-out OS lookup can outlive the request. Keep the sole transport
        // permit until both have ended, while releasing callers at the deadline.
        req.on("close", () => {
          closed = true;
          release();
        });
        req.on("error", () => finish(true));
        req.end();
      } catch {
        closed = true;
        release();
        finish(true);
      }
    });
  };
}
