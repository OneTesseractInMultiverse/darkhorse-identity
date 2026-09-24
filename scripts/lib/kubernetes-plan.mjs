import {
  settings,
  operatorArgs,
  operatorWorkload,
} from "./deployment-plan.mjs";
import { pod, servingPod, labels, proxy } from "./kubernetes-pods.mjs";
import { policies } from "./kubernetes-network.mjs";
const fields = [
  "namespace",
  "origin",
  "image",
  "edgeImage",
  "backendNamespace",
  "ingressNamespace",
];
export function configuration(input) {
  if (
    !input ||
    typeof input !== "object" ||
    Array.isArray(input) ||
    Object.keys(input).length !== fields.length ||
    fields.some((k) => typeof input[k] !== "string") ||
    Object.keys(input).some((k) => !fields.includes(k))
  )
    throw new Error(
      "Supply exactly the documented nonsecret Kubernetes configuration fields.",
    );
  for (const key of ["namespace", "backendNamespace", "ingressNamespace"])
    if (
      !/^[a-z][a-z0-9-]{0,30}[a-z0-9]$/.test(input[key]) ||
      input[key] === "default" ||
      input[key].startsWith("kube-")
    )
      throw new Error("Dedicated bounded namespaces are required.");
  if (
    new Set([input.namespace, input.backendNamespace, input.ingressNamespace])
      .size !== 3
  )
    throw new Error(
      "Use separate application, dependency and ingress namespaces.",
    );
  for (const key of ["image", "edgeImage"])
    if (
      !/^[a-z0-9][a-z0-9.:/_-]{0,190}@sha256:[a-f0-9]{64}$/.test(input[key]) ||
      input[key].includes("://")
    )
      throw new Error(
        "Images must use an explicit repository and immutable digest.",
      );
  settings({
    name: input.namespace,
    origin: input.origin,
    image: input.image.split("@")[1],
    edgeImage: input.edgeImage.split("@")[1],
  });
  return Object.fromEntries(fields.map((k) => [k, input[k]]));
}
export function budgets() {
  return {
    pods: 4,
    databaseConnections: 20,
    limiterConnections: 16,
    cacheConnections: 8,
  };
}
export function application(input) {
  const c = configuration(input),
    metadata = (name) => ({ name, namespace: c.namespace });
  return {
    apiVersion: "v1",
    kind: "List",
    items: [
      {
        apiVersion: "v1",
        kind: "Namespace",
        metadata: {
          name: c.namespace,
          labels: {
            "app.kubernetes.io/name": "darkhorse",
            "app.kubernetes.io/managed-by": "darkhorse",
            "pod-security.kubernetes.io/enforce": "restricted",
            "pod-security.kubernetes.io/enforce-version": "v1.36",
            "pod-security.kubernetes.io/audit": "restricted",
            "pod-security.kubernetes.io/audit-version": "v1.36",
            "pod-security.kubernetes.io/warn": "restricted",
            "pod-security.kubernetes.io/warn-version": "v1.36",
          },
        },
      },
      ...["runtime", "operator", "migrator"].map((role) => ({
        apiVersion: "v1",
        kind: "ServiceAccount",
        metadata: metadata(`darkhorse-${role}`),
        automountServiceAccountToken: false,
      })),
      {
        apiVersion: "v1",
        kind: "ResourceQuota",
        metadata: metadata("darkhorse-budget"),
        spec: {
          hard: {
            pods: "4",
            "requests.cpu": "2",
            "limits.cpu": "9",
            "requests.memory": "2Gi",
            "limits.memory": "3Gi",
            "count/jobs.batch": "1",
          },
        },
      },
      {
        apiVersion: "v1",
        kind: "ConfigMap",
        metadata: metadata("darkhorse-edge"),
        data: { Caddyfile: proxy },
      },
      {
        apiVersion: "apps/v1",
        kind: "Deployment",
        metadata: metadata("darkhorse"),
        spec: {
          replicas: 2,
          revisionHistoryLimit: 2,
          minReadySeconds: 5,
          progressDeadlineSeconds: 180,
          strategy: {
            type: "RollingUpdate",
            rollingUpdate: { maxSurge: 1, maxUnavailable: 0 },
          },
          selector: { matchLabels: labels("server") },
          template: {
            metadata: { labels: labels("server") },
            spec: servingPod(c),
          },
        },
      },
      {
        apiVersion: "v1",
        kind: "Service",
        metadata: metadata("darkhorse"),
        spec: {
          type: "ClusterIP",
          selector: labels("server"),
          ports: [{ name: "https", port: 443, targetPort: "https" }],
        },
      },
      {
        apiVersion: "policy/v1",
        kind: "PodDisruptionBudget",
        metadata: metadata("darkhorse"),
        spec: { minAvailable: 1, selector: { matchLabels: labels("server") } },
      },
      ...policies(c),
    ],
  };
}
export function operatorJob(input, command, name, args = []) {
  const c = configuration(input);
  if (
    ![
      "migrate",
      "migration-inspect",
      "limiter-status",
      "limiter-fence",
      "limiter-activate",
      "signing-status",
      "redis-status",
    ].includes(command) ||
    !/^[a-z][a-z0-9-]{0,30}[a-z0-9]$/.test(name)
  )
    throw new Error(
      "Use a supported one-shot command and unique bounded job name.",
    );
  const role = operatorWorkload(command, args);
  const spec = pod(c, role);
  spec.restartPolicy = "Never";
  spec.containers[0].name = role;
  spec.containers[0].args = [
    ...operatorArgs(command, args),
    ...(command === "migration-inspect" ? [] : ["--yes"]),
  ];
  return {
    apiVersion: "batch/v1",
    kind: "Job",
    metadata: { name, namespace: c.namespace },
    spec: {
      backoffLimit: 0,
      podReplacementPolicy: "Failed",
      parallelism: 1,
      completions: 1,
      activeDeadlineSeconds: 120,
      ttlSecondsAfterFinished: 3600,
      template: { metadata: { labels: labels(role) }, spec },
    },
  };
}
