import { successful } from "./security-command.mjs";
import { resourceLabels, resourceIds, ownerLabel } from "./boundary-ci.mjs";

async function inventory(owner, kind, invoke) {
  const args =
    kind === "container"
      ? ["ps", "--all", "--quiet", "--no-trunc"]
      : ["network", "ls", "--quiet", "--no-trunc"];
  return resourceIds(
    await invoke(
      "docker",
      [...args, "--filter", `label=${ownerLabel}=${owner}`],
      { timeout: 10_000 },
    ),
  );
}
export async function cleanupBoundary(owner, invoke = successful) {
  resourceLabels(owner);
  if (owner === undefined)
    throw new Error("Cleanup requires an explicit owner.");
  const containers = await inventory(owner, "container", invoke);
  if (containers.length)
    await invoke("docker", ["rm", "--force", "--volumes", ...containers], {
      timeout: 20_000,
    });
  const networks = await inventory(owner, "network", invoke);
  if (networks.length)
    await invoke("docker", ["network", "rm", ...networks], { timeout: 20_000 });
  if (
    (await inventory(owner, "container", invoke)).length ||
    (await inventory(owner, "network", invoke)).length
  )
    throw new Error("Owned boundary resources remain after cleanup.");
  return {
    status: "verified",
    containersRemoved: containers.length,
    networksRemoved: networks.length,
  };
}
