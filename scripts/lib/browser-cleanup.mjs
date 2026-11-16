// Attempt all owned teardown operations; retain both exercise and cleanup errors.
export async function closeBrowserResources(operations, original) {
  const errors = [];
  for (const close of operations) {
    try {
      await close();
    } catch (error) {
      errors.push(error);
    }
  }
  if (errors.length)
    throw new AggregateError(
      original === undefined ? errors : [original, ...errors],
      "Browser resource cleanup failed.",
    );
}
