const allowed = {
  "darkhorse-domain": [],
  "darkhorse-application": ["darkhorse-domain"],
};

export function boundaryErrors(packages, paths) {
  return [...dependencyErrors(packages), ...frontendErrors(paths)];
}

function dependencyErrors(packages) {
  return packages.flatMap((pkg) =>
    pkg.name in allowed
      ? pkg.dependencies
          .filter((dep) => !allowed[pkg.name].includes(dep.name))
          .map((dep) => `${pkg.name} cannot depend on ${dep.name}`)
      : [],
  );
}

function frontendErrors(paths) {
  return paths
    .filter(
      (path) =>
        path.startsWith("apps/console/src/") &&
        (/(?:\+server|\.server|\.remote)\.[jt]s$/.test(path) ||
          path.includes("/server/") ||
          /\.(test|spec)\.[jt]s$/.test(path)),
    )
    .map((path) => `Disallowed frontend source location: ${path}`);
}
