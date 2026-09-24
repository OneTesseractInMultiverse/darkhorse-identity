import { createdIdentity, readyIdentity } from "./kubernetes-account-plan.mjs";
export async function runAccountPod(expected, effects) {
  const created = await effects.create(expected);
  const uid = createdIdentity(created, expected);
  effects.report(`Account Pod UID: ${uid}`);
  let result,
    cleanupFailed = false;
  try {
    await effects.ready();
    readyIdentity(await effects.inspect(), expected, uid);
    result = await effects.execute();
  } finally {
    try {
      await effects.remove(uid);
    } catch {
      cleanupFailed = true;
      effects.report(
        "Account Pod cleanup could not be confirmed. Inspect the printed Pod name and UID before manual cleanup. Cleanup does not establish whether an account change committed.",
      );
    }
  }
  return cleanupFailed ? { ...result, code: result.code || 1 } : result;
}
