import assert from "node:assert/strict";
import { Agent } from "node:https";
import { setTimeout as delay } from "node:timers/promises";
import { measureArrivals } from "./benchmark-arrival-phase.mjs";
import { mkdir, mkdtemp, writeFile, appendFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { call } from "./provider-browser.mjs";
import { registerResource, authorizeResource } from "./resource-fixture.mjs";
import { manage, health } from "./reference-client.mjs";
import { benchmarkProfile, phaseSummary } from "./benchmark-model.mjs";
import { runLoad } from "./benchmark-load.mjs";
import { metadata, snapshot } from "./benchmark-observations.mjs";

function expectation(options, fixture, reduced = false) {
  return {
    active: true,
    iss: options.origin,
    aud: fixture.app.audience,
    sub: options.principal,
    client_id: fixture.app.client,
    scope: "openid operate",
    capabilities: reduced
      ? [fixture.app.read]
      : [fixture.app.read, fixture.app.write],
  };
}
async function provision(options, profile) {
  const { browser, origin, password } = options;
  const context = await browser.newContext(),
    page = await context.newPage();
  await page.goto(origin);
  await page.getByLabel("Email address").fill("browser@example.com");
  await page.getByLabel("Password", { exact: true }).fill(password);
  const loginStart = performance.now();
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page.getByRole("heading", { name: "Welcome, Browser." }).waitFor();
  const loginMs = performance.now() - loginStart;
  const jwks = await call(page, "/jwks");
  assert.equal(jwks.status, 200);
  const setup = { ...options, page, context, call, keys: jwks.body.keys };
  const fixtures = [],
    ssoMs = [];
  for (let index = 0; index < profile.clients; index++) {
    const app = await registerResource(setup, `Benchmark ${index}`);
    const start = performance.now();
    const token = await authorizeResource(setup, app);
    ssoMs.push(performance.now() - start);
    fixtures.push({ app, token: token.access_token });
  }
  await context.close();
  return { fixtures, sso: { loginMs, authorizationCodeFlowsMs: ssoMs } };
}
function selection(options, fixtures, index, diverse = true) {
  const client = diverse ? index % fixtures.length : 0;
  return {
    client,
    epoch: "steady",
    expected: expectation(options, fixtures[client]),
  };
}
function request(options, fixtures, agent, selected) {
  if (selected.client === "health")
    return health(options.origin, options.ca, agent);
  const fixture = fixtures[selected.client];
  return manage(
    options.origin,
    options.ca,
    "introspect",
    fixture.app.credential.introspection_client_id,
    selected.invalidSecret ? "00".repeat(32) : fixture.app.credential.secret,
    fixture.token,
    "access_token",
    {},
    agent,
  );
}
async function phase(
  state,
  name,
  count,
  concurrency,
  select,
  agent,
  concurrentChange,
) {
  const clock = () => performance.now() - state.started;
  let begin;
  const dispatched = new Promise((resolve) => {
    begin = resolve;
  });
  const perform = async (selected, index) => {
    const response = request(state.options, state.fixtures, agent, selected);
    if (index === concurrency - 1) begin();
    return response;
  };
  const load = runLoad({ count, concurrency, select, perform, clock });
  const change = concurrentChange
    ? (async () => {
        await dispatched;
        const startMs = clock();
        await concurrentChange();
        return { startMs, acknowledgedMs: clock() };
      })()
    : Promise.resolve(null);
  // Drain every started request even when the change fails, before fixture cleanup.
  const [loaded, changed] = await Promise.allSettled([load, change]);
  if (loaded.status === "rejected") throw loaded.reason;
  if (changed.status === "rejected") throw changed.reason;
  const { rows, wallMs } = loaded.value;
  const summary = phaseSummary(name, concurrency, rows, wallMs, changed.value);
  await recordPhase(state, name, rows, summary);
  return summary;
}
async function recordPhase(state, name, rows, summary) {
  state.report.phases.push(summary);
  await appendFile(
    join(state.directory, "requests.jsonl"),
    rows.map((row) => JSON.stringify({ phase: name, ...row })).join("\n") +
      "\n",
    { mode: 0o600 },
  );
  await save(state);
  console.log(
    `${name}: ${summary.attempts} attempts, authorized=${summary.outcomes.authorized}, unavailable=${summary.outcomes.unavailable}, violations=${summary.outcomes.violation}, p95=${summary.allLatencyMs?.p95.toFixed(2) ?? "n/a"}ms`,
  );
  assert.equal(
    summary.outcomes.violation,
    0,
    "Benchmark authority mismatch; inspect redacted results.",
  );
  assert.equal(
    summary.outcomes.error + summary.outcomes.transport_error,
    0,
    "Benchmark request errors; inspect redacted results.",
  );
  assert.ok(
    summary.outcomes.authorized +
      summary.outcomes.healthy +
      summary.outcomes.denied >
      0,
    "No usable benchmark responses.",
  );
}
async function steady(state, agent) {
  const { options, fixtures, profile } = state;
  const choose = (index) => selection(options, fixtures, index);
  // New connections, not cold database buffers: provisioning has already warmed storage.
  await phase(state, "first-pass-new-tls", profile.clients, 1, choose, false);
  await phase(state, "warmup", 64, 8, choose, agent);
  for (const concurrency of profile.concurrency) {
    await phase(
      state,
      `repeated-c${concurrency}`,
      profile.requests,
      concurrency,
      (index) => selection(options, fixtures, index, false),
      agent,
    );
    await phase(
      state,
      `diverse-c${concurrency}`,
      profile.requests,
      concurrency,
      choose,
      agent,
    );
  }
  const noise = noisySelection(choose);
  await phase(
    state,
    "noisy-invalid-client",
    profile.requests,
    32,
    noise,
    agent,
  );
}
function noisySelection(choose) {
  return (index) => {
    if (index % 8 === 0)
      return { client: "health", epoch: "steady", expected: { status: 200 } };
    const selected = choose(index);
    return index % 4 === 0
      ? selected
      : { ...selected, invalidSecret: true, expected: { status: 401 } };
  };
}
async function permissionReduction(state, agent, measure) {
  const { options, fixtures, profile } = state;
  let epoch = "overlapping";
  const choose = () => ({
    client: 0,
    epoch,
    expected: {
      ...expectation(options, fixtures[0], true),
      ...(epoch === "overlapping"
        ? { alternatives: [[fixtures[0].app.read, fixtures[0].app.write]] }
        : {}),
    },
  });
  await measure(
    state,
    "permission-reduction-concurrent",
    profile.requests,
    8,
    choose,
    agent,
    async () => {
      await options.runSql(
        `DELETE FROM role_capabilities WHERE role_id='${fixtures[0].app.role}' AND capability_id='${fixtures[0].app.write}'`,
      );
      epoch = "after-commit";
    },
  );
  const reduced = await phase(
    state,
    "permission-reduction-after-commit",
    64,
    8,
    choose,
    agent,
  );
  assert.ok(
    reduced.outcomes.authorized > 0,
    "No successful post-commit permission checks.",
  );
}
async function revocation(state, agent, measure) {
  const { options, fixtures, profile } = state;
  const second = fixtures[1];
  let epoch = "overlapping";
  const revokeChoice = () => ({
    client: 1,
    epoch,
    expected:
      epoch === "after-commit"
        ? { active: false }
        : { ...expectation(options, second), allowInactive: true },
  });
  await measure(
    state,
    "revocation-concurrent",
    profile.requests,
    8,
    revokeChoice,
    agent,
    async () => {
      const response = await manage(
        options.origin,
        options.ca,
        "revoke",
        second.app.client,
        second.app.secret,
        second.token,
        "access_token",
        {},
        agent,
      );
      assert.equal(response.status, 200);
      epoch = "after-commit";
    },
  );
  const revoked = await phase(
    state,
    "revocation-after-commit",
    64,
    8,
    revokeChoice,
    agent,
  );
  assert.ok(
    revoked.outcomes.denied > 0,
    "No successful post-commit revocation checks.",
  );
}
async function changes(state, agent, measure = phase) {
  await permissionReduction(state, agent, measure);
  await revocation(state, agent, measure);
  await phase(
    state,
    "unaffected-resource",
    32,
    1,
    () => selection(state.options, state.fixtures, 2),
    agent,
  );
}
async function pacedPhase(state, name, select, agent, rate, change) {
  const settings = { ...state.profile.arrivals, rate };
  const { rows, summary } = await measureArrivals({
    name,
    settings,
    change,
    clock: () => performance.now() - state.started,
    sleep: delay,
    select,
    perform: (selected) =>
      request(state.options, state.fixtures, agent, selected),
  });
  await recordPhase(state, name, rows, summary);
  console.log(
    `  scheduled=${summary.scheduled}, driver-late=${summary.generatorDrops.late}, driver-full=${summary.generatorDrops.full}, authorized scheduled p95=${summary.authorizedScheduledLatencyMs?.p95.toFixed(2) ?? "n/a"}ms`,
  );
  return summary;
}
async function arrivalWorkloads(state, agent) {
  const { options, fixtures, profile } = state;
  const choose = (index) => selection(options, fixtures, index);
  await phase(state, "warmup", 64, 8, choose, agent);
  for (const rate of profile.arrivals.rates)
    await pacedPhase(state, `arrival-diverse-r${rate}`, choose, agent, rate);
  await pacedPhase(
    state,
    "arrival-noisy-invalid-client",
    noisySelection(choose),
    agent,
    profile.arrivals.noiseRate,
  );
  const measure = (state, name, _count, _concurrency, select, agent, change) =>
    pacedPhase(
      state,
      `arrival-${name}`,
      select,
      agent,
      profile.arrivals.changeRate,
      change,
    );
  await changes(state, agent, measure);
}
async function save(state) {
  await writeFile(
    join(state.directory, "report.json"),
    JSON.stringify(state.report, null, 2) + "\n",
    { mode: 0o600 },
  );
}
export async function benchmarkBrowser(options) {
  const profile = benchmarkProfile(process.env.BENCH_PROFILE ?? "smoke");
  await mkdir(resolve(".local/benchmarks"), { recursive: true, mode: 0o700 });
  const directory = await mkdtemp(resolve(".local/benchmarks/run-"));
  const report = {
    schema: 2,
    status: "incomplete",
    timestamp: new Date().toISOString(),
    profile,
    metadata: await metadata(options),
    budgets: {
      latency: null,
      capacity: null,
      errorRate: null,
      freshness:
        "No stale grants for checks dispatched after committed reduction/revocation acknowledgement.",
    },
    phases: [],
  };
  const state = {
    options,
    directory,
    report,
    profile,
    started: performance.now(),
  };
  await save(state);
  console.log(`Benchmark reports: ${directory}`);
  const agent = new Agent({
    keepAlive: true,
    maxSockets: 128,
    maxFreeSockets: 128,
  });
  try {
    const provisioned = await provision(options, profile);
    state.fixtures = provisioned.fixtures;
    report.sso = provisioned.sso;
    report.before = await snapshot(options);
    if (profile.arrivals) await arrivalWorkloads(state, agent);
    else {
      await steady(state, agent);
      await changes(state, agent);
    }
    report.after = await snapshot(options);
    report.status = "passed";
  } finally {
    agent.destroy();
    await save(state);
  }
}
