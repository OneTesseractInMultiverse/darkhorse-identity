// Real subprocess evidence is intentionally separate from isolated unit tests.
import assert from "node:assert/strict";
import { runBenchmarkCommand } from "./lib/benchmark-command.mjs";
const secret = "fixture-private-input";
const run = (source, options = {}) =>
  runBenchmarkCommand(process.execPath, ["-e", source], {
    env: {},
    input: secret,
    timeoutMs: 2000,
    ...options,
  });
assert.equal(
  await run(
    "let data='';process.stdin.on('data',x=>data+=x);process.stdin.on('end',()=>process.stdout.write(data==='fixture-private-input'?'received':'wrong'));",
  ),
  "received",
);
assert.equal(
  await run(
    "process.stdout.write(Buffer.from([0xc3]));setTimeout(()=>process.stdout.write(Buffer.from([0xa9])),20);",
  ),
  "é",
);
await assert.rejects(
  run("process.stderr.write('fixture-private-input');process.exitCode=2;"),
  {
    message:
      "Benchmark operator failed; inspect protected audit evidence before retrying.",
  },
);
assert.equal(
  await run("process.stderr.write('{\\\"ok\\\":false}');process.exitCode=1;", {
    expectedExitCodes: [0, 1],
  }),
  '{"ok":false}',
);
await assert.rejects(
  run(
    "process.stdout.write('unexpected');process.stderr.write('{\\\"ok\\\":false}');process.exitCode=1;",
    { expectedExitCodes: [0, 1] },
  ),
  { message: "Benchmark operator wrote to an unexpected output stream." },
);
await assert.rejects(
  run("process.stderr.write('unexpected failure');process.exitCode=1;"),
  /Benchmark operator failed/,
);
for (const stream of ["stdout", "stderr"])
  await assert.rejects(
    run(`process.${stream}.write('x'.repeat(65537));setInterval(()=>{},100);`),
    /output limit exceeded/,
  );
await assert.rejects(
  run("setInterval(()=>{},100);", { timeoutMs: 30 }),
  /deadline exceeded/,
);
await assert.rejects(
  runBenchmarkCommand("/nonexistent-darkhorse-benchmark-command", [], {
    env: {},
    input: secret,
  }),
  /could not start/,
);
const aborted = new AbortController();
aborted.abort();
await assert.rejects(
  run("process.exit(0)", { signal: aborted.signal }),
  /interrupted/,
);
const active = new AbortController();
const interrupted = run("setInterval(()=>{},100);", { signal: active.signal });
setTimeout(() => active.abort(), 50);
await assert.rejects(interrupted, /interrupted/);
console.log(
  "Benchmark native process input, output bounds, exit, deadline, cancellation and redaction passed.",
);
