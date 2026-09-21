import { join } from "node:path";
import { writeFile, chmod } from "node:fs/promises";
export async function createCertificates(directory, host, command) {
  const caKey = join(directory, "ca.key"),
    ca = join(directory, "ca.pem");
  await command(
    "openssl",
    [
      "req",
      "-x509",
      "-newkey",
      "rsa:3072",
      "-nodes",
      "-keyout",
      caKey,
      "-out",
      ca,
      "-days",
      "30",
      "-subj",
      "/CN=Darkhorse local stack CA",
      "-addext",
      "basicConstraints=critical,CA:TRUE",
      "-addext",
      "keyUsage=critical,keyCertSign,cRLSign",
    ],
    { capture: true },
  );
  await chmod(caKey, 0o600);
  for (const [name, dns] of [
    ["edge", host],
    ["postgres", "postgres"],
    ["cache", "cache"],
    ["limiter", "limiter"],
  ])
    await leaf(directory, name, dns, command);
}
async function leaf(directory, name, dns, command) {
  const file = (ext) => join(directory, `${name}.${ext}`);
  await command(
    "openssl",
    [
      "req",
      "-new",
      "-newkey",
      "rsa:2048",
      "-nodes",
      "-keyout",
      file("key"),
      "-out",
      file("csr"),
      "-subj",
      `/CN=${dns}`,
    ],
    { capture: true },
  );
  await writeFile(
    file("ext"),
    `basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:${dns}\n`,
    { mode: 0o600 },
  );
  await command(
    "openssl",
    [
      "x509",
      "-req",
      "-in",
      file("csr"),
      "-CA",
      join(directory, "ca.pem"),
      "-CAkey",
      join(directory, "ca.key"),
      "-CAserial",
      join(directory, "ca.srl"),
      "-CAcreateserial",
      "-out",
      file("pem"),
      "-days",
      "7",
      "-extfile",
      file("ext"),
    ],
    { capture: true },
  );
  await chmod(file("key"), 0o600);
}
