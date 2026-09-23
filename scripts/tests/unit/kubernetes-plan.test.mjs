import test from "node:test";
import assert from "node:assert/strict";
import {
  configuration,
  application,
  operatorJob,
  budgets,
} from "../../lib/kubernetes-plan.mjs";
const input = {
  namespace: "darkhorse-trial",
  origin: "https://identity.example.com",
  image: "registry.example.com/darkhorse@sha256:" + "a".repeat(64),
  edgeImage: "registry.example.com/edge@sha256:" + "b".repeat(64),
  backendNamespace: "identity-state",
  ingressNamespace: "identity-ingress",
};
test("deployment input rejects mutable images, ambient namespaces and unbounded configuration", () => {
  for (const change of [
    { namespace: "default" },
    { namespace: "kube-system" },
    { namespace: "../prod" },
    { origin: "http://identity.example.com" },
    { image: "darkhorse:latest" },
    { extra: "secret" },
    { backendNamespace: "bad space" },
    { namespace: "x".repeat(33) },
    { backendNamespace: input.namespace },
    { ingressNamespace: input.backendNamespace },
    {
      image:
        "registry.example.com/" + "a".repeat(300) + "@sha256:" + "a".repeat(64),
    },
    { image: "https://registry.example.com/x@sha256:" + "a".repeat(64) },
    { edgeImage: "edge:latest" },
    { origin: "https://user:secret@identity.example.com" },
  ])
    assert.throws(() => configuration({ ...input, ...change }));
  for (const invalid of [null, [], {}, { ...input, image: undefined }])
    assert.throws(() => configuration(invalid));
  assert.equal(configuration(input).namespace, input.namespace);
});
test("replicas, terminating pods and operator processes fit an explicit bounded envelope", () => {
  const c = configuration(input);
  const list = application(c);
  const d = list.items.find((x) => x.kind === "Deployment");
  assert.equal(d.spec.replicas, 2);
  assert.deepEqual(d.spec.strategy.rollingUpdate, {
    maxSurge: 1,
    maxUnavailable: 0,
  });
  const p = d.spec.template.spec;
  assert.equal(p.automountServiceAccountToken, false);
  for (const container of p.containers) {
    assert.equal(container.securityContext.runAsNonRoot, true);
    assert.equal(container.securityContext.readOnlyRootFilesystem, true);
    assert.deepEqual(container.securityContext.capabilities.drop, ["ALL"]);
    assert.equal(container.securityContext.allowPrivilegeEscalation, false);
  }
  assert.ok(!JSON.stringify(p).includes("operator-secrets"));
  const runtime = p.containers.find((x) => x.name === "api");
  assert.equal(runtime.livenessProbe.httpGet.path, "/health/live");
  assert.equal(runtime.readinessProbe.httpGet.path, "/health/ready");
  assert.ok(runtime.env.every((x) => !x.value?.includes("postgres://")));
  assert.deepEqual(budgets(), {
    pods: 4,
    databaseConnections: 20,
    limiterConnections: 16,
    cacheConnections: 8,
  });
  const policies = list.items.filter((x) => x.kind === "NetworkPolicy");
  assert.ok(
    policies.some(
      (x) => x.spec.egress?.length === 0 && x.spec.ingress?.length === 0,
    ),
  );
  assert.ok(!JSON.stringify(list).includes('kind":"Secret'));
});
test("operator jobs disable configured retries and reject arbitrary command text", () => {
  const c = configuration(input);
  const job = operatorJob(c, "migrate", "migration-1");
  assert.equal(job.spec.backoffLimit, 0);
  assert.equal(job.spec.parallelism, 1);
  assert.equal(job.spec.completions, 1);
  assert.equal(job.spec.template.spec.restartPolicy, "Never");
  assert.equal(job.spec.activeDeadlineSeconds, 120);
  assert.deepEqual(job.spec.template.spec.containers[0].args, [
    "migrate",
    "--yes",
  ]);
  assert.ok(JSON.stringify(job).includes("migrator-secrets"));
  for (const command of ["serve", "bootstrap", "sh", "migrate; echo secret"])
    assert.throws(() => operatorJob(c, command, "job-1"));
});

test("all admitted runtime or operator pods fit the advertised connection envelope", () => {
  const c = configuration(input);
  const items = application(c).items;
  const runtime = items.find((v) => v.kind === "Deployment").spec.template.spec;
  const operator = operatorJob(c, "redis-status", "status-1").spec.template
    .spec;
  const quota = Number(
    items.find((v) => v.kind === "ResourceQuota").spec.hard.pods,
  );
  const ceiling = budgets();
  for (let servers = 0; servers <= quota; servers++) {
    const jobs = quota - servers;
    for (const [name, limit] of [
      ["DARKHORSE_DATABASE_POOL_SIZE", ceiling.databaseConnections],
      ["DARKHORSE_REDIS_LIMITER_CONNECTIONS", ceiling.limiterConnections],
      ["DARKHORSE_REDIS_CACHE_CONNECTIONS", ceiling.cacheConnections],
    ]) {
      const size = (pod) =>
        Number(pod.containers[0].env.find((e) => e.name === name).value);
      assert.ok(servers * size(runtime) + jobs * size(operator) <= limit);
    }
  }
});

test("policy peers require both the selected namespace and the correct dependency label", () => {
  const policies = application(input).items.filter(
    (v) => v.kind === "NetworkPolicy",
  );
  for (const name of ["darkhorse-runtime", "darkhorse-operator"]) {
    const policy = policies.find((v) => v.metadata.name === name).spec;
    assert.equal(policy.egress.length, 4);
    for (const [i, component] of ["postgres", "cache", "limiter"].entries()) {
      const rule = policy.egress[i + 1];
      assert.equal(rule.to.length, 1);
      assert.deepEqual(rule.to[0].namespaceSelector.matchLabels, {
        "kubernetes.io/metadata.name": input.backendNamespace,
      });
      assert.deepEqual(rule.to[0].podSelector.matchLabels, {
        "app.kubernetes.io/name": "darkhorse",
        "app.kubernetes.io/component": component,
      });
      assert.deepEqual(rule.ports, [
        { port: i === 0 ? 5432 : 6379, protocol: "TCP" },
      ]);
    }
  }
  const operator = policies.find(
    (v) => v.metadata.name === "darkhorse-operator",
  );
  assert.deepEqual(operator.spec.ingress, []);
  const ingress = policies.find((v) => v.metadata.name === "darkhorse-runtime")
    .spec.ingress;
  assert.deepEqual(ingress, [
    {
      from: [
        {
          namespaceSelector: {
            matchLabels: {
              "kubernetes.io/metadata.name": input.ingressNamespace,
            },
          },
        },
      ],
      ports: [{ port: 8443, protocol: "TCP" }],
    },
  ]);
});

test("migration owner secrets never reach operators or runtime and migration has only database dependencies", () => {
  const migration = operatorJob(input, "migrate", "migration-1").spec.template;
  const operator = operatorJob(input, "limiter-fence", "fence-1").spec.template;
  const runtime = application(input).items.find((v) => v.kind === "Deployment")
    .spec.template;
  for (const [template, role, file] of [
    [migration, "migrator", "owner-db"],
    [operator, "operator", "operator-db"],
    [runtime, "runtime", "runtime-db"],
  ]) {
    const spec = template.spec;
    assert.equal(spec.serviceAccountName, `darkhorse-${role}`);
    const secret = spec.volumes.find((v) => v.name === "identity").secret;
    assert.equal(secret.secretName, `darkhorse-${role}-secrets`);
    assert.equal(
      spec.containers[0].env.find(
        (v) => v.name === "DARKHORSE_DATABASE_URL_FILE",
      ).value,
      `/run/secrets/${file}`,
    );
    // Explicit projection also prevents mounting accidentally co-located secrets.
    const keys = secret.items.map((v) => v.key);
    assert.ok(keys.includes(file));
    assert.equal(keys.includes("owner-db"), role === "migrator");
    if (role === "migrator") {
      assert.deepEqual(keys, ["owner-db", "ca"]);
      assert.ok(
        spec.containers[0].env.every(
          (v) => !/REDIS|LOGIN|SIGNING/.test(v.name),
        ),
      );
    }
  }
  const policy = application(input).items.find(
    (v) =>
      v.kind === "NetworkPolicy" && v.metadata.name === "darkhorse-migrator",
  ).spec;
  assert.deepEqual(policy.ingress, []);
  assert.equal(policy.egress.length, 2);
  assert.deepEqual(policy.egress[1].ports, [{ port: 5432, protocol: "TCP" }]);
});
