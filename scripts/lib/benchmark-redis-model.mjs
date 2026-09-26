// Fixed, credential-free projection of Redis INFO. Never retain the source response.
export function redisMetrics(text) {
  if (typeof text !== "string" || text.length > 65536)
    throw new Error("Invalid Redis benchmark metrics.");
  const fields = {
    used_memory: "usedBytes",
    used_memory_peak: "peakUsedBytes",
    total_commands_processed: "commands",
    used_cpu_user: "userCpuSeconds",
    used_cpu_sys: "systemCpuSeconds",
  };
  const result = {};
  for (const line of text.split(/\r?\n/)) {
    const colon = line.indexOf(":");
    const key = line.slice(0, colon);
    if (!Object.hasOwn(fields, key)) continue;
    const value = line.slice(colon + 1),
      number = Number(value);
    const decimal = key.startsWith("used_cpu_");
    if (
      Object.hasOwn(result, fields[key]) ||
      !(decimal ? /^(0|[1-9]\d*)(\.\d+)?$/ : /^(0|[1-9]\d*)$/).test(value) ||
      !Number.isFinite(number) ||
      number > Number.MAX_SAFE_INTEGER
    )
      throw new Error("Invalid Redis benchmark metrics.");
    result[fields[key]] = number;
  }
  if (
    Object.keys(result).length !== Object.keys(fields).length ||
    result.peakUsedBytes < result.usedBytes
  )
    throw new Error("Invalid Redis benchmark metrics.");
  return result;
}
