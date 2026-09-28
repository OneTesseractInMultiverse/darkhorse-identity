import { test } from "node:test";
import assert from "node:assert/strict";
import { promisify } from "node:util";
import { execFile } from "node:child_process";
import {
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

const execute = promisify(execFile);
const trustScript = resolve(import.meta.dirname, "../../trust.sh");
const certificate = ".local/pki/pki/authorities/local/root.crt";

async function fixture({ host = "Darwin", includeCertificate = true } = {}) {
  const root = await mkdtemp(join(tmpdir(), "darkhorse-trust-test-"));
  const workspace = join(root, "workspace");
  const binaries = join(root, "bin");
  const log = join(root, "calls.log");
  const certificatePath = join(workspace, certificate);
  await mkdir(workspace, { recursive: true });
  await mkdir(binaries, { recursive: true });
  if (includeCertificate) {
    await mkdir(dirname(certificatePath), { recursive: true });
    await writeFile(
      certificatePath,
      "source-defined public root certificate\n",
    );
  }
  const commands = {
    uname: `#!/bin/sh\nprintf '%s\\n' '${host}'\n`,
    openssl: `#!/bin/sh\nprintf 'openssl:%s\\n' "$*" >> "$DARKHORSE_TRUST_TEST_LOG"\nprintf 'subject=CN=Test Root\\nSHA256 Fingerprint=AA:BB\\n'\n`,
    security: `#!/bin/sh\nprintf 'security:%s\\n' "$*" >> "$DARKHORSE_TRUST_TEST_LOG"\n`,
  };
  for (const [name, contents] of Object.entries(commands)) {
    const path = join(binaries, name);
    await writeFile(path, contents);
    await chmod(path, 0o700);
  }
  return {
    root,
    workspace,
    log,
    certificatePath,
    run: (operation) =>
      execute("/bin/bash", [trustScript, operation], {
        cwd: workspace,
        env: {
          ...process.env,
          HOME: root,
          PATH: `${binaries}:${process.env.PATH}`,
          DARKHORSE_TRUST_TEST_LOG: log,
        },
      }),
  };
}

test("trust and untrust use the project root and preserve certificate material", async () => {
  const testFixture = await fixture();
  try {
    const trusted = await testFixture.run("trust");
    assert.match(trusted.stdout, /SHA256 Fingerprint=AA:BB/);
    assert.deepEqual(
      (await readFile(testFixture.log, "utf8")).trim().split("\n"),
      [
        `openssl:x509 -in ${certificate} -noout -subject -fingerprint -sha256`,
        `security:add-trusted-cert -r trustRoot -p ssl -k ${join(testFixture.root, "Library/Keychains/login.keychain-db")} ${certificate}`,
      ],
    );

    await testFixture.run("untrust");
    assert.deepEqual(
      (await readFile(testFixture.log, "utf8")).trim().split("\n").slice(2),
      [
        `openssl:x509 -in ${certificate} -noout -subject -fingerprint -sha256`,
        `security:remove-trusted-cert ${certificate}`,
      ],
    );
    assert.equal(
      await readFile(testFixture.certificatePath, "utf8"),
      "source-defined public root certificate\n",
    );
  } finally {
    await rm(testFixture.root, { recursive: true, force: true });
  }
});

test("missing certificate fails before invoking host trust tools", async () => {
  const testFixture = await fixture({ includeCertificate: false });
  try {
    await assert.rejects(
      testFixture.run("trust"),
      /Run make https-setup first/,
    );
    await assert.rejects(readFile(testFixture.log));
  } finally {
    await rm(testFixture.root, { recursive: true, force: true });
  }
});

test("non-macOS trust automation fails before printing or changing trust", async () => {
  const testFixture = await fixture({ host: "Linux" });
  try {
    await assert.rejects(
      testFixture.run("trust"),
      /Host trust automation currently supports macOS only/,
    );
    await assert.rejects(readFile(testFixture.log));
    assert.equal(
      await readFile(testFixture.certificatePath, "utf8"),
      "source-defined public root certificate\n",
    );
  } finally {
    await rm(testFixture.root, { recursive: true, force: true });
  }
});
