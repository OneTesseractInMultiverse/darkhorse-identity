import assert from "node:assert/strict";
import { Agent } from "node:https";
import { setTimeout as delay } from "node:timers/promises";
import { operatorFixture } from "./benchmark-operator-fixture.mjs";
import { measureOperatorPhase } from "./benchmark-operator-load.mjs";
import {
  operatorSummary,
  operatorLimits,
  postCommitRevocationAccepted,
} from "./benchmark-operator-model.mjs";
import { measureArrivals } from "./benchmark-arrival-phase.mjs";
import { mkdir, mkdtemp, writeFile, appendFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { call } from "./provider-browser.mjs";
import { registerResource, authorizeResource } from "./resource-fixture.mjs";
import { manage, health } from "./reference-client.mjs";
import { phaseSummary } from "./benchmark-model.mjs";
import { runLoad } from "./benchmark-load.mjs";
import { verifyPhaseProfile } from "./benchmark-profile-consistency.mjs";
import { profiler } from "./benchmark-profiler.mjs";
import { metadata, snapshot } from "./benchmark-observations.mjs";
import { lifecycleFixtureStatements } from "./benchmark-cleanup-fixture.mjs";
import {
  cleanupDeletePlanSql,
  cleanupSelectionPlanSql,
  cleanupTableStatisticsSql,
  summarizeCleanupQueryPlan,
} from "./benchmark-cleanup-plan.mjs";

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
function benchmarkUnixMs() {
  return performance.timeOrigin + performance.now();
}
async function queryDatabaseJson(options, statement) {
  const result = await options.docker([
    "exec",
    options.db.name,
    "psql",
    "-U",
    "postgres",
    "-d",
    "browser_test",
    "-v",
    "ON_ERROR_STOP=1",
    "-qAtc",
    statement,
  ]);
  return JSON.parse(result.stdout.trim());
}
async function cleanupPopulation(options, statement) {
  const population = await queryDatabaseJson(options, statement);
  for (const key of ["expired", "live"])
    if (!Number.isSafeInteger(population[key]) || population[key] < 0)
      throw new Error("Invalid lifecycle benchmark population result.");
  return population;
}
async function prepareLifecycleCleanup(state) {
  const applicationId = state.fixtures[0].app.identity.application_id;
  const statements = lifecycleFixtureStatements(
    applicationId,
    state.profile.lifecycleCleanup,
  );
  await state.options.runSql(statements.seed);
  await state.options.runSql("ANALYZE authorization_requests");
  const before = await cleanupPopulation(state.options, statements.counts);
  assert.equal(before.expired, state.profile.lifecycleCleanup.expiredRows);
  assert.equal(before.live, state.profile.lifecycleCleanup.liveRows);
  state.lifecycleCleanup = {
    countsSql: statements.counts,
    seededAtUnixMs: benchmarkUnixMs(),
    before,
  };
  state.report.lifecycleCleanup = {
    populationBefore: before,
    tableStatisticsBefore: await queryDatabaseJson(
      state.options,
      cleanupTableStatisticsSql,
    ),
    queryPlans: {
      boundedSelection: summarizeCleanupQueryPlan(
        await queryDatabaseJson(state.options, cleanupSelectionPlanSql),
      ),
      boundedDelete: summarizeCleanupQueryPlan(
        await queryDatabaseJson(state.options, cleanupDeletePlanSql),
      ),
    },
    scheduledSweeps: [],
    overlappingPhases: [],
  };
}
async function completeLifecycleCleanup(state) {
  const { options, lifecycleCleanup, cleanupEvents } = state;
  const deadline = performance.now() + 65_000;
  while (
    !cleanupEvents.some(
      (event) =>
        event.observedAtUnixMs >= lifecycleCleanup.seededAtUnixMs &&
        event.status === "ok" &&
        event.deleted > 0,
    ) &&
    performance.now() < deadline
  )
    await delay(100);

  const sweeps = cleanupEvents.filter(
    (event) => event.observedAtUnixMs >= lifecycleCleanup.seededAtUnixMs,
  );
  const successful = sweeps.filter((event) => event.status === "ok");
  assert.ok(
    successful.some((event) => event.deleted > 0),
    "Scheduled cleanup did not report progress against the synthetic population.",
  );
  const after = await cleanupPopulation(options, lifecycleCleanup.countsSql);
  const tableStatisticsAfter = await queryDatabaseJson(
    options,
    cleanupTableStatisticsSql,
  );
  assert.equal(after.live, lifecycleCleanup.before.live);
  assert.ok(after.expired < lifecycleCleanup.before.expired);
  assert.equal(
    lifecycleCleanup.before.expired - after.expired,
    successful.reduce((sum, event) => sum + event.deleted, 0),
    "Scheduled cleanup report and synthetic expired-row progress disagree.",
  );
  assert.ok(successful.some((event) => event.backlogRemaining));

  const overlappingPhases = state.report.phases
    .filter((phase) =>
      sweeps.some(
        (event) =>
          event.observedAtUnixMs >= phase.measurementWindowUnixMs.startedAt &&
          event.observedAtUnixMs <= phase.measurementWindowUnixMs.endedAt,
      ),
    )
    .map((phase) => phase.name);
  state.report.lifecycleCleanup = {
    populationBefore: lifecycleCleanup.before,
    populationAfter: after,
    tableStatisticsBefore: state.report.lifecycleCleanup.tableStatisticsBefore,
    tableStatisticsAfter,
    queryPlans: state.report.lifecycleCleanup.queryPlans,
    expiredRowsRemoved: lifecycleCleanup.before.expired - after.expired,
    scheduledSweeps: sweeps,
    overlappingPhases,
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
  await state.observer?.before();
  const startedAt = benchmarkUnixMs();
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
  summary.measurementWindowUnixMs = {
    startedAt,
    endedAt: benchmarkUnixMs(),
  };
  await recordPhase(state, name, rows, summary);
  return summary;
}
async function recordPhase(state, name, rows, summary) {
  if (state.observer) {
    summary.profiling = await state.observer.after();
    verifyPhaseProfile(summary.profiling, summary);
  }
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
async function pacedPhase(state, name, select, agent, rate, change, operator) {
  await state.observer?.before();
  const startedAt = benchmarkUnixMs();
  const settings = { ...state.profile.arrivals, rate };
  const load = () =>
    measureArrivals({
      name,
      settings,
      change,
      clock: () => performance.now() - state.started,
      sleep: delay,
      select,
      perform: (selected) =>
        request(state.options, state.fixtures, agent, selected),
    });
  const { rows, summary } = operator?.invoke
    ? await measureOperatorPhase({
        load,
        durationMs: settings.durationMs,
        clock: () => performance.now() - state.started,
        sleep: delay,
        invoke: operator.invoke,
        details: state.profile.details,
      })
    : await load();
  summary.measurementWindowUnixMs = {
    startedAt,
    endedAt: benchmarkUnixMs(),
  };
  if (operator?.mutation)
    summary.operators = operatorSummary(
      [
        {
          operation: "account.revoke_all",
          startMs: summary.change.startMs,
          endMs: summary.change.acknowledgedMs,
        },
      ],
      rows,
    );
  await recordPhase(state, name, rows, summary);
  if (operator)
    assert.ok(
      summary.operators.commands.every((command) => command.requestsDuring > 0),
      "No HTTPS dispatch overlapped native CLI execution.",
    );
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
async function operatorWorkloads(state, agent) {
  const { options, fixtures, profile } = state;
  const fixture = await operatorFixture(
    options,
    fixtures[0].app,
    profile.details,
  );
  state.report.operatorPopulation = fixture.population;
  const choose = (index) => selection(options, fixtures, index);
  await phase(state, "warmup", 64, 8, choose, agent);
  const rate = profile.arrivals.rate;
  await pacedPhase(state, "operator-control-before", choose, agent, rate);
  const reads = await pacedPhase(
    state,
    "operator-read-bursts",
    choose,
    agent,
    rate,
    undefined,
    { invoke: fixture.read },
  );
  await pacedPhase(state, "operator-control-after", choose, agent, rate);
  await operatorRevocation(state, agent, fixture, choose);
  state.report.operatorAudit = await fixture.verify(reads.operators.commands);
}
async function operatorRevocation(state, agent, fixture, choose) {
  let committed = false;
  const revoked = (index) => ({
    ...choose(index),
    epoch: committed ? "after-commit" : "overlapping",
    expected: committed
      ? { active: false }
      : { ...choose(index).expected, allowInactive: true },
  });
  await pacedPhase(
    state,
    "operator-revocation-concurrent",
    revoked,
    agent,
    state.profile.arrivals.rate,
    async () => {
      await fixture.revoke();
      committed = true;
    },
    { mutation: true },
  );
  const after = await phase(
    state,
    "operator-revocation-after-commit",
    64,
    8,
    revoked,
    agent,
  );
  assert.ok(
    postCommitRevocationAccepted(after),
    "Post-commit probes must all fail closed without drops or stale access.",
  );
}
async function save(state) {
  await writeFile(
    join(state.directory, "report.json"),
    JSON.stringify(state.report, null, 2) + "\n",
    { mode: 0o600 },
  );
}
export async function benchmarkBrowser(options, profile) {
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
    cleanupEvents: options.cleanupEvents ?? [],
    started: performance.now(),
  };
  await save(state);
  console.log(`Benchmark: ${profile.name}, pool=${options.poolSize}`);
  console.log(`Benchmark reports: ${directory}`);
  const agent = new Agent({
    keepAlive: true,
    maxSockets: 128,
    maxFreeSockets: 128,
  });
  try {
    const provisioned = await provision(options, profile);
    state.fixtures = provisioned.fixtures;
    if (profile.lifecycleCleanup) await prepareLifecycleCleanup(state);
    report.sso = provisioned.sso;
    if (profile.operators)
      report.operatorLimits = operatorLimits(
        options.poolSize,
        profile.restrictedDatabase,
      );
    report.before = await snapshot(options);
    if (profile.profiling) {
      state.observer = await profiler(options);
      report.profiling = { enabled: true, postgres: state.observer.settings };
    }
    if (profile.operators) await operatorWorkloads(state, agent);
    else if (profile.arrivals) await arrivalWorkloads(state, agent);
    else {
      await steady(state, agent);
      await changes(state, agent);
    }
    if (profile.lifecycleCleanup) await completeLifecycleCleanup(state);
    report.after = await snapshot(options);
    report.status = "passed";
  } finally {
    agent.destroy();
    await save(state);
  }
}
