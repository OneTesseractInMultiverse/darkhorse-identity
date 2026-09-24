import test from "node:test";
import assert from "node:assert/strict";
import { runAccountPod } from "../../lib/kubernetes-account-lifecycle.mjs";
const uid = "00000000-0000-0000-0000-000000000123";
const expected = {
  metadata: { name: "test", namespace: "identity-app" },
  spec: { containers: [{ image: "immutable" }] },
};
function fixture(failure, result = { code: 0, uncertain: false }) {
  const calls = [];
  const response = {
    ...expected,
    metadata: { ...expected.metadata, uid },
    status: {
      phase: "Running",
      containerStatuses: [
        { name: "api", ready: true, restartCount: 0, state: { running: {} } },
      ],
    },
  };
  const effect =
    (name, value) =>
    async (...args) => {
      calls.push([name, ...args]);
      if (name === failure) throw new Error("injected");
      return value;
    };
  return {
    calls,
    effects: {
      create: effect("create", response),
      ready: effect("ready"),
      inspect: effect("inspect", response),
      execute: effect("execute", result),
      remove: effect("remove"),
      report: (message) => calls.push(["report", message]),
    },
  };
}
test("one Pod receives one execution and cleanup is conditional on its returned UID", async () => {
  for (const code of [0, 1, 3, 124, 130]) {
    const outcome = { code, uncertain: code >= 124 },
      f = fixture(undefined, outcome);
    assert.deepEqual(await runAccountPod(expected, f.effects), outcome);
    assert.deepEqual(
      f.calls.map((v) => v[0]),
      ["create", "report", "ready", "inspect", "execute", "remove"],
    );
    assert.equal(f.calls.at(-1)[1], uid);
  }
});
test("ambiguous creation is never adopted, deleted by name or retried", async () => {
  const f = fixture("create");
  await assert.rejects(runAccountPod(expected, f.effects));
  assert.deepEqual(
    f.calls.map((v) => v[0]),
    ["create"],
  );
});
test("preflight or execution failures clean up the original Pod without retrying", async () => {
  for (const failure of ["ready", "inspect", "execute"]) {
    const f = fixture(failure);
    await assert.rejects(runAccountPod(expected, f.effects));
    assert.equal(f.calls.at(-1)[0], "remove");
    assert.equal(
      f.calls.filter((v) => v[0] === "execute").length,
      failure === "execute" ? 1 : 0,
    );
  }
});
test("cleanup failure is visible and never replaces the command failure status", async () => {
  for (const code of [0, 3, 130]) {
    const f = fixture("remove", { code, uncertain: code === 130 });
    const result = await runAccountPod(expected, f.effects);
    assert.equal(result.code, code || 1);
    assert.equal(f.calls.at(-1)[0], "report");
    assert.equal(f.calls.filter((v) => v[0] === "execute").length, 1);
  }
});
