// Test-only reference receiver; never included in the deployed Rust server.
import { createHash } from "node:crypto";
import { logoutTokenKeyId, validateLogoutToken } from "./reference-client.mjs";

export function parseLogoutRequest({ method, type, body }) {
  if (
    method !== "POST" ||
    typeof type !== "string" ||
    !/^application\/x-www-form-urlencoded(?:;\s*charset=utf-8)?$/i.test(type) ||
    typeof body !== "string" ||
    Buffer.byteLength(body) > 16384
  )
    throw new Error("Invalid logout request.");
  const form = new URLSearchParams(body);
  if (form.size !== 1 || !form.get("logout_token"))
    throw new Error("Invalid logout request.");
  return form.get("logout_token");
}
function validate(request, context) {
  const token = parseLogoutRequest(request);
  return {
    claims: validateLogoutToken(token, context.keys, context),
    fingerprint: createHash("sha256").update(token).digest("hex"),
  };
}
export function receiveLogout(request, context, store) {
  let input;
  try {
    input = validate(request, context);
  } catch {
    return 400;
  }
  try {
    return acknowledgment(store.accept(input.claims, input.fingerprint));
  } catch {
    return 503;
  }
}
function acknowledgment(result) {
  if (result === "accepted" || result === "duplicate") return 204;
  return result === "invalid" ? 400 : 503;
}
export async function receiveLogoutWithJwks(
  request,
  context,
  store,
  source,
  clock,
) {
  let kid;
  try {
    kid = logoutTokenKeyId(parseLogoutRequest(request), {
      ...context,
      now: clock(),
    });
  } catch {
    return 400;
  }
  let keys;
  try {
    keys = await source.getKeys(context.issuer, kid);
  } catch {
    return 503;
  }
  return receiveLogout(request, { ...context, keys, now: clock() }, store);
}
