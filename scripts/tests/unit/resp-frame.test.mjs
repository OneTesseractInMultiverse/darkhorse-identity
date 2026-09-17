import test from "node:test";
import assert from "node:assert/strict";
import { frame } from "../../lib/resp-frame.mjs";
test("fault proxy parses fragmented, combined, error and nested RESP2 frames", () => {
  const bytes = Buffer.from("*2\r\n$3\r\nGET\r\n$1\r\nx\r\n:1\r\n");
  const first = frame(bytes);
  assert.deepEqual(first.value, ["GET", "x"]);
  assert.equal(frame(bytes, first.end).value, "1");
  for (let n = 0; n < first.end; n++)
    assert.equal(frame(bytes.subarray(0, n)), null);
  assert.deepEqual(frame(Buffer.from("-NOSCRIPT missing\r\n")).value, {
    error: "NOSCRIPT missing",
  });
  assert.equal(frame(Buffer.from("$-1\r\n")).value, null);
  assert.deepEqual(frame(Buffer.from("*1\r\n*1\r\n+OK\r\n")).value, [["OK"]]);
  for (const text of [
    "?x\r\n",
    "$x\r\n",
    "$-2\r\n",
    "$9000000\r\n",
    "$1\r\nxzz",
  ])
    assert.throws(() => frame(Buffer.from(text)));
  assert.throws(() => frame(Buffer.alloc(8 * 1024 * 1024 + 1)));
  assert.throws(() => frame(Buffer.from("*1\r\n".repeat(10) + "+OK\r\n")));
});
