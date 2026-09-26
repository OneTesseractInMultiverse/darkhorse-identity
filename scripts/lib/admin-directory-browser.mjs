import { verifyDirectoryPresentation } from "./directory-localization-browser.mjs";
import assert from "node:assert/strict";
import { expect } from "@playwright/test";
import { resolve } from "node:path";
const target = "00000000-0000-0000-0000-000000000a01";
const app = "00000000-0000-0000-0000-000000000a02";
const role = "00000000-0000-0000-0000-000000000a03";
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
        noStore: response.headers.get("cache-control"),
      };
    },
    { path, body },
  );
}
async function search(page, value) {
  await page.getByLabel("Buscar usuarios").fill(value);
  await page.getByRole("button", { name: "Buscar", exact: true }).click();
  await page.getByRole("table").waitFor();
}
async function saved(page) {
  await page
    .getByRole("status")
    .filter({ hasText: "Cambio guardado." })
    .waitFor();
  await page.getByRole("table").waitFor();
}
export async function verifyDirectory(page, origin, principal, runSql) {
  // Constants and bootstrap-produced UUID belong only to this disposable database.
  assert.match(principal, /^[a-f0-9-]{36}$/);
  await runSql(
    `INSERT INTO principals(id,email,first_name,last_name) VALUES('${target}','directory@example.com','Directory','Fixture'); INSERT INTO applications(id,name,owner_id,active) VALUES('${app}','Directory application','${principal}',true); INSERT INTO roles(id,name) VALUES('${role}','Directory reader'); INSERT INTO role_applications(application_id,role_id) VALUES('${app}','${role}');`,
  );
  await verifyDirectoryPresentation(page, origin);
  await page.goto(`${origin}/console/users`);
  await page
    .getByRole("combobox", { name: /^(Language|Idioma)$/ })
    .selectOption("es");
  await page
    .getByRole("heading", { name: "Directorio de usuarios", exact: true })
    .waitFor();
  await page.getByRole("table").waitFor();
  const list = await call(page, "/api/admin/users");
  assert.equal(list.status, 200);
  assert.equal(list.noStore, "no-store");
  await page.screenshot({
    path: resolve(".local/directory-desktop.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.ok(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  );
  await page.screenshot({
    path: resolve(".local/directory-mobile.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 1280, height: 900 });
  await search(page, "directory@");
  const view = page.getByRole("button", { name: "Ver a Directory Fixture" });
  await view.click();
  await page.getByRole("dialog").waitFor();
  await page.keyboard.press("Escape");
  // The native dialog leaves the accessibility tree before its queued close event.
  // Wait for component removal and the promised focus restoration.
  await page.locator("dialog").waitFor({ state: "detached" });
  await expect(view).toBeFocused();
  await view.click();
  await page
    .getByRole("button", { name: "Editar nombre", exact: true })
    .click();
  await page.getByLabel("Nombre", { exact: true }).fill("Directory updated");
  assert.equal(
    await page
      .getByLabel("Nombre", { exact: true })
      .evaluate((node) => node === document.activeElement),
    true,
  );
  await page.screenshot({
    path: resolve(".local/directory-edit.png"),
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Guardar nombre", exact: true })
    .click();
  await saved(page);
  let user = (await call(page, `/api/admin/users/${target}`)).body;
  assert.equal(user.first_name, "Directory updated");
  assert.equal(user.email, "directory@example.com");
  const deactivate = page.getByRole("button", {
    name: "Desactivar a Directory updated Fixture",
  });
  await deactivate.click();
  await page.getByRole("button", { name: "Cancelar", exact: true }).click();
  assert.equal(
    (await call(page, `/api/admin/users/${target}`)).body.active,
    true,
  );
  await deactivate.click();
  await page.getByRole("button", { name: "Confirmar desactivación" }).click();
  await saved(page);
  assert.equal(
    (await call(page, `/api/admin/users/${target}`)).body.active,
    false,
  );
  await page
    .getByRole("button", { name: "Reactivar a Directory updated Fixture" })
    .click();
  await page.getByRole("button", { name: "Confirmar reactivación" }).click();
  await saved(page);
  await runSql(
    `DO $$ BEGIN IF (SELECT credential_epoch FROM principals WHERE id='${target}')<>1 THEN RAISE EXCEPTION 'revocation epoch changed on reactivation'; END IF; END $$;`,
  );
  await page
    .getByRole("button", { name: "Asignar acceso a Directory updated Fixture" })
    .click();
  await page.getByLabel("Aplicación", { exact: true }).selectOption(app);
  await page.getByLabel("Rol", { exact: true }).selectOption(role);
  await page.getByLabel("Asignar este rol").check();
  await page.getByRole("button", { name: "Guardar acceso" }).click();
  await saved(page);
  let access = (
    await call(page, `/api/admin/users/${target}/access?application=${app}`)
  ).body;
  assert.equal(access.roles[0].assigned, true);
  const wrong = await call(page, `/api/admin/users/${target}`, {
    revision: access.user.revision,
    change: {
      kind: "role",
      application_id: "00000000-0000-0000-0000-000000000aff",
      role_id: role,
      assigned: false,
      policy_revision: access.policy_revision,
    },
  });
  assert.equal(wrong.status, 400);
  assert.equal(
    (await call(page, `/api/admin/users/${target}/access?application=${app}`))
      .body.roles[0].assigned,
    true,
  );
  // A concurrent edit makes the form stale; the failed write cannot overwrite it.
  await page
    .getByRole("button", { name: "Ver a Directory updated Fixture" })
    .click();
  await page
    .getByRole("button", { name: "Editar nombre", exact: true })
    .click();
  await page.getByLabel("Nombre", { exact: true }).fill("Stale replacement");
  await runSql(
    `UPDATE principals SET first_name='Concurrent',revision=revision+1 WHERE id='${target}'`,
  );
  await page.getByRole("button", { name: "Guardar nombre" }).click();
  await page.getByRole("alert").filter({ hasText: "cambió" }).waitFor();
  assert.equal(
    await page
      .getByRole("button", { name: "Ver a Directory updated Fixture" })
      .isDisabled(),
    true,
  );
  assert.equal(
    (await call(page, `/api/admin/users/${target}`)).body.first_name,
    "Concurrent",
  );
  await page.getByRole("button", { name: "Actualizar directorio" }).click();
  await page
    .getByRole("button", { name: "Ver a Concurrent Fixture" })
    .waitFor();
  // A read denial clears private records already rendered by the page.
  const directoryRequests = (url) => url.pathname === "/api/admin/users";
  await page.route(directoryRequests, (route) =>
    route.fulfill({
      status: 403,
      contentType: "application/json",
      body: '{"error":"administrator_required"}',
    }),
  );
  await page.getByRole("button", { name: "Actualizar directorio" }).click();
  await page
    .getByRole("alert")
    .filter({ hasText: "Se requiere acceso de administrador" })
    .waitFor();
  assert.equal(await page.getByRole("table").count(), 0);
  await page.unroute(directoryRequests);
  await page
    .getByRole("combobox", { name: /^(Language|Idioma)$/ })
    .selectOption("en");
  await page.goto(origin);
  await page.getByRole("heading", { name: "Welcome, Browser." }).waitFor();
  console.log(
    "Console directory search, profile edits, confirmation, role binding, stale writes, revocation epoch, focus and responsive checks passed.",
  );
}
