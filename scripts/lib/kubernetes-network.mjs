import { labels } from "./kubernetes-pods.mjs";
const namespace = (name) => ({
  namespaceSelector: { matchLabels: { "kubernetes.io/metadata.name": name } },
});
const peer = (name, component) => ({
  ...namespace(name),
  podSelector: { matchLabels: labels(component) },
});
const port = (port, protocol = "TCP") => ({ port, protocol });
export function backendPolicies(c) {
  return [
    ["postgres", 5432, ["server", "operator", "migrator", "account"]],
    ["cache", 6379, ["server", "operator"]],
    ["limiter", 6379, ["server", "operator", "account"]],
  ].map(([component, number, clients]) => ({
    apiVersion: "networking.k8s.io/v1",
    kind: "NetworkPolicy",
    metadata: {
      name: `darkhorse-${component}-ingress`,
      namespace: c.backendNamespace,
    },
    spec: {
      podSelector: { matchLabels: labels(component) },
      policyTypes: ["Ingress"],
      ingress: [
        {
          from: clients.map((role) => peer(c.namespace, role)),
          ports: [port(number)],
        },
      ],
    },
  }));
}
export function policies(c) {
  const make = (name, spec) => ({
    apiVersion: "networking.k8s.io/v1",
    kind: "NetworkPolicy",
    metadata: { name, namespace: c.namespace },
    spec,
  });
  const dependencies = [
    {
      to: [
        {
          ...namespace("kube-system"),
          podSelector: { matchLabels: { "k8s-app": "kube-dns" } },
        },
      ],
      ports: [port(53), port(53, "UDP")],
    },
    ...["postgres", "cache", "limiter"].map((role) => ({
      to: [peer(c.backendNamespace, role)],
      ports: [port(role === "postgres" ? 5432 : 6379)],
    })),
  ];
  return [
    make("darkhorse-default-deny", {
      podSelector: {},
      policyTypes: ["Ingress", "Egress"],
      ingress: [],
      egress: [],
    }),
    make("darkhorse-runtime", {
      podSelector: { matchLabels: labels("server") },
      policyTypes: ["Ingress", "Egress"],
      ingress: [{ from: [namespace(c.ingressNamespace)], ports: [port(8443)] }],
      egress: dependencies,
    }),
    make("darkhorse-migrator", {
      podSelector: { matchLabels: labels("migrator") },
      policyTypes: ["Ingress", "Egress"],
      ingress: [],
      egress: dependencies.slice(0, 2),
    }),
    make("darkhorse-operator", {
      podSelector: { matchLabels: labels("operator") },
      policyTypes: ["Ingress", "Egress"],
      ingress: [],
      egress: dependencies,
    }),
    make("darkhorse-account", {
      podSelector: { matchLabels: labels("account") },
      policyTypes: ["Ingress", "Egress"],
      ingress: [],
      egress: [dependencies[0], dependencies[1], dependencies[3]],
    }),
  ];
}
