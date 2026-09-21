import { setTimeout as delay } from "node:timers/promises";
export async function provisionBucket(
  command,
  endpoint,
  bucket,
  access,
  secret,
) {
  const common = `url = "${endpoint}/${bucket}"\naws-sigv4 = "aws:amz:us-east-1:s3"\nuser = "${access}:${secret}"\nsilent\nshow-error\nconnect-timeout = 2\nmax-time = 5\n`;
  // The HTTP health route may become live before S3 has initialized its metadata.
  for (let i = 0; i < 120; i++) {
    const probe = await command("curl", ["--config", "-"], {
      input: common + 'head\nwrite-out = "%{http_code}"\n',
      capture: true,
      acceptFailure: true,
    });
    const status = probe.stdout.slice(-3);
    if (probe.code === 0 && status === "200") return;
    if (probe.code === 0 && status === "404") {
      await command("curl", ["--config", "-"], {
        input: common + 'request = "PUT"\nheader = "Content-Length: 0"\nfail\n',
        capture: true,
      });
      return;
    }
    if (probe.code === 0 && !["503", "000"].includes(status))
      throw new Error("S3 bucket readiness or authorization failed.");
    await delay(250);
  }
  throw new Error("S3 metadata did not become ready.");
}
