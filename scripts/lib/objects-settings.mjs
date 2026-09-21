import { createHash } from "node:crypto";
export function objectSettings(text) {
  if (text.length > 1024) throw new Error("Invalid local object settings.");
  const value = JSON.parse(text);
  if (
    !value ||
    Object.keys(value).sort().join(",") !== "access,secret" ||
    ![value.access, value.secret].every(
      (v) => typeof v === "string" && /^[a-f0-9]{64}$/.test(v),
    ) ||
    value.access === value.secret
  )
    throw new Error("Invalid local object credentials.");
  return value;
}
export function objectEnvironment(settings, port = "9009") {
  return {
    DARKHORSE_OBJECTS_ENABLED: "true",
    DARKHORSE_OBJECTS_ENDPOINT: `http://127.0.0.1:${port}`,
    DARKHORSE_OBJECTS_BUCKET: "darkhorse-local",
    DARKHORSE_OBJECTS_REGION: "us-east-1",
    DARKHORSE_OBJECTS_ACCESS_KEY: settings.access,
    DARKHORSE_OBJECTS_SECRET_KEY: settings.secret,
    DARKHORSE_OBJECTS_LOCAL_HTTP: "true",
  };
}
export function objectProject(directory) {
  return `darkhorse-objects-${createHash("sha256").update(directory).digest("hex").slice(0, 10)}`;
}
