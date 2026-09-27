export function javascriptBundleBudget({ maximumGzipBytes, currentGzipBytes }) {
  if (
    !Number.isSafeInteger(maximumGzipBytes) ||
    maximumGzipBytes < 0 ||
    !Number.isSafeInteger(currentGzipBytes) ||
    currentGzipBytes < 0
  ) {
    throw new Error("Production JavaScript budget measurement is invalid.");
  }

  return {
    maximumGzipBytes,
    remainingGzipBytes: maximumGzipBytes - currentGzipBytes,
    withinBudget: currentGzipBytes <= maximumGzipBytes,
  };
}
