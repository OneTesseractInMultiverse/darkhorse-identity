import assert from "node:assert/strict";
import { request } from "node:https";
import { resolve } from "node:path";
import {
  verifyApplicationOverview,
  verifyClientFormGuidance,
} from "./catalog-presentation-browser.mjs";
async function call(page, path, body) {
  return page.evaluate(
    async ({ path, body }) => {
      const response = await fetch(path, {
        method: body === undefined ? "GET" : "POST",
        cache: "no-store",
        headers: {
          "content-type": "application/json",
          "x-darkhorse-csrf": "1",
        },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      });
      return {
        status: response.status,
        body: await response.json(),
        cache: response.headers.get("cache-control"),
      };
    },
    { path, body },
  );
}
async function saved(page, access = false) {
  await page
    .getByRole("status")
    .filter({
      hasText: access ? "Política de acceso guardada." : "Cambio guardado.",
    })
    .waitFor();
  await page.getByRole("table").waitFor();
}
async function create(page, kind, name) {
  await page
    .getByRole("button", {
      name: `Crear ${{ application: "aplicación", resource: "recurso", scope: "ámbito", role: "rol", client: "cliente" }[kind]}`,
      exact: true,
    })
    .click();
  await page.getByLabel("Nombre", { exact: true }).fill(name);
}
async function close(page) {
  await page.getByRole("button", { name: "Cerrar", exact: true }).click();
  await page.getByRole("dialog").waitFor({ state: "detached" });
}
async function edge(page, button, label) {
  await page.getByRole("button", { name: button, exact: true }).click();
  await page
    .getByRole("button", { name: `Seleccionar ${label}`, exact: true })
    .click();
  await page
    .getByRole("button", { name: "Confirmar cambio", exact: true })
    .click();
  await saved(page, true);
}
export async function verifyCatalog(page, origin, ca) {
  await page.goto(`${origin}/console/applications`);
  await page
    .getByRole("combobox", { name: /^(Language|Idioma)$/ })
    .selectOption("es");
  await create(page, "application", "Console portal");
  await page
    .getByRole("button", {
      name: "Seleccionar browser@example.com",
      exact: true,
    })
    .click();
  await page.getByRole("button", { name: "Crear", exact: true }).click();
  await saved(page);
  const apps = await call(
    page,
    "/api/admin/catalog/applications?search=Console",
  );
  assert.equal(apps.status, 200);
  assert.equal(apps.cache, "no-store");
  const app = apps.body.items[0];
  assert.equal(typeof app.revision, "string");
  await page.screenshot({
    path: resolve(".local/catalog-applications.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.ok(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  );
  await page.screenshot({
    path: resolve(".local/catalog-mobile.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 1280, height: 900 });
  const route = (kind) => `${origin}/console/${kind}?application_id=${app.id}`;
  await page
    .getByRole("button", { name: "Ver Console portal", exact: true })
    .click();
  await verifyApplicationOverview(page, app.id, "es");
  await page
    .getByRole("button", { name: "Editar aplicación", exact: true })
    .click();
  await page
    .getByLabel("Nombre", { exact: true })
    .fill("Console portal edited");
  await page
    .getByRole("button", { name: "Guardar cambios", exact: true })
    .click();
  await saved(page);
  await page.goto(route("resources"));
  await create(page, "resource", "Console API");
  await page.getByRole("button", { name: "Crear", exact: true }).click();
  await saved(page);
  await page.goto(route("scopes"));
  await create(page, "scope", "console.read");
  await page
    .getByRole("button", { name: "Seleccionar Console API", exact: true })
    .click();
  await page.getByRole("button", { name: "Crear", exact: true }).click();
  await saved(page);
  await page.goto(route("capabilities"));
  await page
    .getByRole("button", { name: "Crear capacidad", exact: true })
    .click();
  await page.getByLabel("Clave de permiso").fill("console.read");
  await page
    .getByLabel("Significado", { exact: true })
    .fill("Read console records");
  await page.getByRole("button", { name: "Crear", exact: true }).click();
  await saved(page, true);
  await page.goto(route("roles"));
  await create(page, "role", "Console reader");
  await page.getByRole("button", { name: "Crear", exact: true }).click();
  await saved(page, true);
  await page
    .getByRole("button", { name: "Ver Console reader", exact: true })
    .click();
  await edge(page, "Añadir capacidad", "console.read");
  await page.goto(route("resources"));
  await page
    .getByRole("button", { name: "Ver Console API", exact: true })
    .click();
  await edge(page, "Añadir capacidad", "console.read");
  await page.goto(route("scopes"));
  await page
    .getByRole("button", { name: "Ver console.read", exact: true })
    .click();
  await edge(page, "Añadir capacidad", "console.read");
  await page.goto(route("clients"));
  await create(page, "client", "Console web");
  await verifyClientFormGuidance(page, "es");
  await page
    .getByLabel("URLs de retorno")
    .fill("https://console.example/callback");
  await page
    .getByRole("button", { name: "Seleccionar Console API", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Seleccionar console.read", exact: true })
    .click();
  await page.getByRole("button", { name: "Crear", exact: true }).click();
  await page.getByLabel("Secreto del cliente", { exact: true }).waitFor();
  const secret = await page
    .getByLabel("Secreto del cliente", { exact: true })
    .inputValue();
  assert.match(secret, /^[a-f0-9]{64}$/);
  await page
    .getByRole("button", { name: "He guardado el secreto", exact: true })
    .click();
  assert.equal(
    await page.getByLabel("Secreto del cliente", { exact: true }).count(),
    0,
  );
  assert.equal(
    await page.evaluate((secret) => {
      return JSON.stringify([
        Object.entries(localStorage),
        Object.entries(sessionStorage),
      ]).includes(secret);
    }, secret),
    false,
  );
  const clients = await call(
    page,
    `/api/admin/catalog/clients?application_id=${app.id}`,
  );
  const client = clients.body.items[0];
  const clientPath = `/api/admin/console/applications/${app.id}/clients/${client.id}`;
  const detail = await call(page, clientPath);
  assert.equal(detail.body.client_secret, undefined);
  assert.ok(!JSON.stringify(detail).includes(secret));
  await page
    .getByRole("button", { name: "Ver Console web", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Editar cliente", exact: true })
    .click();
  await page
    .getByLabel("URLs de retorno")
    .fill("https://console.example/new-callback");
  await page
    .getByRole("button", { name: "Guardar cambios", exact: true })
    .click();
  await saved(page);
  await page
    .getByRole("button", { name: "Ver Console web", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Rotar secreto", exact: true })
    .click();
  await page
    .getByLabel("Vigencia simultánea del secreto anterior (segundos)")
    .fill("60");
  await page
    .getByRole("button", { name: "Confirmar rotación", exact: true })
    .click();
  await page.getByLabel("Secreto del cliente", { exact: true }).waitFor();
  assert.notEqual(
    await page.getByLabel("Secreto del cliente", { exact: true }).inputValue(),
    secret,
  );
  await page
    .getByRole("button", { name: "He guardado el secreto", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Ver Console web", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Retirar secreto", exact: true })
    .first()
    .click();
  await page
    .getByRole("button", { name: "Confirmar retiro", exact: true })
    .click();
  await saved(page);
  await page
    .getByRole("button", { name: "Ver Console web", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Rotar secreto", exact: true })
    .click();
  const beforeLoss = await call(page, clientPath);
  let mutations = 0,
    forwardingFailed = false;
  await page.route("**/api/admin/console/registration", async (route) => {
    mutations++;
    try {
      await discardResponse(route.request(), ca);
    } catch {
      forwardingFailed = true;
    }
    await route.abort("failed");
  });
  await page
    .getByRole("button", { name: "Confirmar rotación", exact: true })
    .click();
  await page
    .getByRole("alert")
    .filter({ hasText: "No se pudo confirmar" })
    .waitFor();
  assert.equal(mutations, 1);
  assert.equal(forwardingFailed, false);
  const afterLoss = await call(page, clientPath);
  assert.equal(
    BigInt(afterLoss.body.revision),
    BigInt(beforeLoss.body.revision) + 1n,
  );
  assert.ok(
    await page
      .getByRole("button", { name: "Ver Console web", exact: true })
      .isDisabled(),
  );
  assert.equal(
    await page.getByLabel("Secreto del cliente", { exact: true }).count(),
    0,
  );
  await page.unroute("**/api/admin/console/registration");
  await page
    .getByRole("button", { name: "Actualizar catálogo", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Ver Console web", exact: true })
    .click();
  await close(page);
  // A stale policy snapshot must not remove an application binding.
  await page.goto(route("roles"));
  await page
    .getByRole("button", { name: "Ver Console reader", exact: true })
    .click();
  const catalog = await call(page, "/api/admin/catalog/roles");
  assert.equal(
    (
      await call(page, "/api/admin/catalog", {
        policy_revision: catalog.body.policy_revision,
        change: { operation: "create_role", name: "Concurrent reader" },
      })
    ).status,
    200,
  );
  await page
    .getByRole("button", {
      name: "Desvincular Console portal edited",
      exact: true,
    })
    .click();
  await page
    .getByRole("button", { name: "Confirmar cambio", exact: true })
    .click();
  await page.getByRole("alert").filter({ hasText: "cambió" }).waitFor();
  assert.ok(
    await page
      .getByRole("button", { name: "Ver Console reader", exact: true })
      .isDisabled(),
  );
  await page.route("**/api/admin/catalog/roles?**", (route) =>
    route.fulfill({
      status: 403,
      contentType: "application/json",
      body: '{"error":"forbidden"}',
    }),
  );
  await page
    .getByRole("button", { name: "Actualizar catálogo", exact: true })
    .click();
  await page
    .getByRole("alert")
    .filter({ hasText: "Se requiere acceso de administrador" })
    .waitFor();
  assert.equal(await page.getByRole("table").count(), 0);
  await page.unroute("**/api/admin/catalog/roles?**");
  await page
    .getByRole("combobox", { name: /^(Language|Idioma)$/ })
    .selectOption("en");
}

async function discardResponse(original, ca) {
  const headers = await original.allHeaders();
  return new Promise((resolve, reject) => {
    const req = request(
      original.url(),
      { method: original.method(), headers, ca, timeout: 5000 },
      (response) => {
        response.resume();
        response.on("end", () =>
          response.statusCode === 200
            ? resolve()
            : reject(new Error("Unexpected mutation outcome")),
        );
        response.on("error", () =>
          reject(new Error("Mutation response unavailable")),
        );
      },
    );
    req.on("error", () => reject(new Error("Mutation transport unavailable")));
    req.on("timeout", () => req.destroy(new Error("Mutation timed out")));
    req.end(original.postData());
  });
}
