import assert from "node:assert/strict";

async function call(page, path, body, csrf = true) {
  return page.evaluate(
    async ({ path, body, csrf }) => {
      const response = await fetch(path, {
        method: body === undefined ? "GET" : "POST",
        headers: {
          "content-type": "application/json",
          ...(csrf ? { "x-darkhorse-csrf": "1" } : {}),
        },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
      return {
        status: response.status,
        noStore: response.headers.get("cache-control")?.includes("no-store"),
        body: await response.json(),
      };
    },
    { path, body, csrf },
  );
}
async function write(page, body) {
  const response = await call(page, "/api/admin/registration", body);
  assert.equal(response.status, 200);
  assert.equal(response.noStore, true);
  return response.body;
}
export async function verifyRegistration(page, principal) {
  const create = {
    operation: "create_application",
    application: {
      name: "Browser application",
      owner_id: principal,
      active: true,
    },
  };
  assert.equal(
    (await call(page, "/api/admin/registration", create, false)).status,
    403,
  );
  const { record: application } = await write(page, create);
  assert.equal(application.owner_id, principal);
  const { record: resource } = await write(page, {
    operation: "create_resource",
    application_id: application.id,
    name: "Orders API",
  });
  const { record: scope } = await write(page, {
    operation: "create_scope",
    application_id: application.id,
    resource_id: resource.id,
    name: "orders.read",
  });
  const clientInput = {
    operation: "create_client",
    application_id: application.id,
    client: {
      name: "Web client",
      active: true,
      redirect_uris: ["https://app.example/callback"],
      resource_ids: [resource.id],
      scope_ids: [scope.id],
      token_endpoint_auth_method: "client_secret_basic",
    },
  };
  const issued = await write(page, clientInput);
  assert.ok(/^[a-f0-9]{64}$/.test(issued.client_secret));
  const clientPath = `/api/admin/applications/${application.id}/clients/${issued.record.id}`;
  const view = await call(page, clientPath);
  assert.equal(view.status, 200);
  assert.equal(view.body.client_secret, undefined);
  assert.equal(view.body.verifier, undefined);
  assert.deepEqual(view.body.scope_ids, [scope.id]);

  const rotate = {
    operation: "rotate_secret",
    application_id: application.id,
    client_id: issued.record.id,
    revision: 0,
    overlap_seconds: 0,
  };
  const rotations = await Promise.all([
    call(page, "/api/admin/registration", rotate),
    call(page, "/api/admin/registration", rotate),
  ]);
  assert.deepEqual(rotations.map((r) => r.status).sort(), [200, 409]);
  const winner = rotations.find((r) => r.status === 200).body;
  assert.ok(winner.client_secret !== issued.client_secret);
  assert.equal(
    rotations.find((r) => r.status === 409).body.client_secret,
    undefined,
  );
  assert.equal(winner.record.id, issued.record.id);
  assert.equal(winner.record.secrets.length, 1);

  const retired = await write(page, {
    operation: "retire_secret",
    application_id: application.id,
    client_id: issued.record.id,
    secret_id: winner.record.secrets[0].id,
    revision: 1,
  });
  assert.deepEqual(retired.record.secrets, []);
  assert.equal(retired.client_secret, undefined);
  const { record: other } = await write(page, create);
  assert.equal(
    (
      await call(page, "/api/admin/registration", {
        ...clientInput,
        application_id: other.id,
      })
    ).status,
    400,
  );
  const invalid = structuredClone(clientInput);
  invalid.client.redirect_uris = ["https://*.example/callback"];
  assert.equal(
    (await call(page, "/api/admin/registration", invalid)).status,
    400,
  );
  // SvelteKit may retain scroll/snapshot state after reload. Credentials must
  // never be written alongside that framework state.
  assert.ok(
    await page.evaluate(
      (secrets) =>
        [localStorage, sessionStorage].every((storage) =>
          Object.values(storage).every((value) =>
            secrets.every((secret) => !value.includes(secret)),
          ),
        ),
      [issued.client_secret, winner.client_secret],
    ),
    "issued client secrets must remain absent from browser storage",
  );
  console.log(
    "HTTPS administrator registration, explicit grants, rotation race, retirement and CSRF checks passed.",
  );
}
