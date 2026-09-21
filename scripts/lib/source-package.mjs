const required = [
  "Cargo.toml",
  "Cargo.lock",
  "Makefile",
  "README.md",
  "SECURITY.md",
  "package.json",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
  "rust-toolchain.toml",
  "CONTRIBUTING.md",
  "docs/engineering.md",
];
const forbidden = new Set([
  ".git",
  "context",
  ".context",
  ".local",
  ".idea",
  "target",
  "node_modules",
  "coverage",
  ".svelte-kit",
  "build",
]);
export function sourceReference(args) {
  const ref = args[0] ?? "HEAD";
  if (
    args.length > 1 ||
    !ref ||
    ref.length > 256 ||
    /[\x00-\x20\x7f]/.test(ref) ||
    ref.startsWith("-")
  ) {
    throw new Error("Provide one explicit Git commit or tree reference.");
  }
  return ref;
}
export function objectId(raw) {
  const revision = raw.trim();
  if (!/^[a-f0-9]{40}$/.test(revision)) throw new Error("Invalid Git object.");
  return revision;
}
function validPath(path) {
  const parts = path.split("/");
  if (
    !path ||
    path.length > 1024 ||
    /[\\\x00-\x1f\x7f:]/.test(path) ||
    parts.some(
      (v) => !v || v === "." || v === ".." || forbidden.has(v.toLowerCase()),
    )
  )
    throw new Error(
      "Source package contains a private, generated or nonportable path.",
    );
  const file = parts.at(-1).toLowerCase();
  if (
    (file.startsWith(".env") && file !== ".env.example") ||
    /\.(pem|key|p12|pfx|der|crt)$/.test(file)
  )
    throw new Error("Credential-shaped files cannot enter the source package.");
  return path;
}
export function sourceTree(raw) {
  if (typeof raw !== "string" || !raw.endsWith("\0"))
    throw new Error("Incomplete source inventory.");
  const paths = raw
    .slice(0, -1)
    .split("\0")
    .map((line) => {
      const match = /^(100644|100755) blob [a-f0-9]{40}\t([^\0]+)$/.exec(line);
      if (!match)
        throw new Error(
          "Only regular source files are supported; links and submodules require review.",
        );
      return validPath(match[2]);
    });
  if (
    new Set(paths).size !== paths.length ||
    required.some((p) => !paths.includes(p))
  )
    throw new Error("Required public source is missing or duplicated.");
  return paths.sort();
}
export function archivePaths(expected, raw) {
  if (typeof raw !== "string" || !raw.endsWith("\n"))
    throw new Error("Incomplete archive inventory.");
  const paths = raw.slice(0, -1).split("\n");
  for (const p of paths) validPath(p.endsWith("/") ? p.slice(0, -1) : p);
  const files = paths.filter((p) => !p.endsWith("/"));
  if (
    new Set(paths).size !== paths.length ||
    files.length !== expected.length ||
    files.some((p) => !expected.includes(p))
  )
    throw new Error("Exported source differs from the reviewed Git tree.");
  return files.length;
}
