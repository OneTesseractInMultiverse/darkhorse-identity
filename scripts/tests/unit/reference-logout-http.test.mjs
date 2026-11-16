import { test } from "node:test";
import assert from "node:assert/strict";
import { readLogoutBody } from "../../lib/reference-logout-http.mjs";

async function* chunks(values) {
  for (const value of values) yield Buffer.from(value);
}
test("shared receiver listener bounds streamed bytes before retaining an overflowing chunk", async () => {
  assert.equal(
    await readLogoutBody(chunks(["logout_", "token=abc"])),
    "logout_token=abc",
  );
  assert.equal(
    (await readLogoutBody(chunks(["a".repeat(16384)]))).length,
    16384,
  );
  await assert.rejects(
    readLogoutBody(chunks(["a".repeat(16000), "b".repeat(385)])),
  );
  await assert.rejects(readLogoutBody(chunks(["é".repeat(8193)])));
});
