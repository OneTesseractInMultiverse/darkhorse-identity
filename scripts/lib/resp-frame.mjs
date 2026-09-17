// Bounded RESP2 framing for the disposable fault-injection proxy.
export function frame(buffer, offset = 0, depth = 0) {
  if (buffer.length > 8 * 1024 * 1024 || depth > 8)
    throw new Error("RESP frame exceeds test proxy bounds.");
  const end = buffer.indexOf("\r\n", offset);
  if (end < 0) return null;
  const type = String.fromCharCode(buffer[offset]);
  const text = buffer.toString("utf8", offset + 1, end);
  let cursor = end + 2;
  if (["+", ":", "-"].includes(type))
    return { end: cursor, value: type === "-" ? { error: text } : text };
  if (!["$", "*"].includes(type) || !/^-?\d+$/.test(text))
    throw new Error("Invalid test proxy frame.");
  const length = Number(text);
  if (!Number.isSafeInteger(length) || length < -1 || length > 8 * 1024 * 1024)
    throw new Error("Invalid test proxy length.");
  if (length === -1) return { end: cursor, value: null };
  if (type === "$") {
    if (buffer.length < cursor + length + 2) return null;
    if (
      buffer.toString("ascii", cursor + length, cursor + length + 2) !== "\r\n"
    )
      throw new Error("Invalid bulk terminator.");
    return {
      end: cursor + length + 2,
      value: buffer.toString("utf8", cursor, cursor + length),
    };
  }
  const values = [];
  for (let i = 0; i < length; i++) {
    const next = frame(buffer, cursor, depth + 1);
    if (!next) return null;
    values.push(next.value);
    cursor = next.end;
  }
  return { end: cursor, value: values };
}
