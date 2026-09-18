import { test } from "node:test";
import assert from "node:assert/strict";
import {
  validateCallback,
  validateIdToken,
} from "../../lib/reference-client.mjs";

test("callback rejects changed state/issuer and duplicate or substituted credentials", () => {
  const expected = {
    issuer: "https://issuer.example",
    state: "state",
    redirect: "https://client.example/callback?fixed=1",
  };
  const good = `${expected.redirect}&code=dc_${"ab".repeat(32)}&state=state&iss=${encodeURIComponent(expected.issuer)}`;
  assert.match(validateCallback(good, expected), /^dc_/);
  for (const value of [
    good + "&state=state",
    good + "&error=denied",
    good.replace("state=state", "state=other"),
    good.replace("fixed=1", "fixed=2"),
    good.replace("client.example", "other.example"),
    good.replace("dc_", "da_"),
    good + "#fragment",
  ]) {
    assert.throws(() => validateCallback(value, expected));
  }
});
test("ID validator refuses unsupported JWT profiles before consulting a key", () => {
  for (const header of [
    { alg: "none", typ: "JWT" },
    { alg: "RS256", typ: "logout+jwt" },
    { alg: "HS256", typ: "JWT" },
    { alg: "RS256", typ: "JWT", jku: "https://evil.example" },
  ]) {
    const jwt = `${Buffer.from(JSON.stringify(header)).toString("base64url")}.e30.AA`;
    assert.throws(() => validateIdToken(jwt, [], {}));
  }
});
