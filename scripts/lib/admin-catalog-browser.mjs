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
      const started = performance.now();
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
        durationMs: performance.now() - started,
      };
    },
    { path, body },
  );
}
function policyFixture(application, config) {
  const identifier = (value) =>
    `00000000-0000-4000-8000-${value.toString(16).padStart(12, "0")}`;
  const makeNodes = (type, count, offset, name, extra = () => ({})) =>
    Array.from({ length: count }, (_, index) => {
      const id = identifier(offset + index);
      return {
        id: `${type}:${id}`,
        type,
        identifier: id,
        name: `${name} ${index}`,
        ...extra(id, index),
      };
    });
  const roles = makeNodes("role", config.roles, 1000, "Role");
  const capabilities = makeNodes(
    "capability",
    config.capabilities,
    2000,
    "Capability",
    (_, index) => ({
      key: `policy:${index}`,
      meaning: `Synthetic bounded graph capability ${index}`,
      retired: false,
    }),
  );
  const resources = makeNodes(
    "resource",
    config.resources,
    3000,
    "Resource",
    (_, index) => ({
      audience: `urn:darkhorse:benchmark:${index}`,
    }),
  );
  const scopes = makeNodes(
    "scope",
    config.scopes,
    4000,
    "Scope",
    (_, index) => ({
      resource_id: resources[index].id,
    }),
  );
  const app = {
    id: application.id,
    name: application.name,
    active: application.active,
  };
  const appNode = {
    id: `application:${application.id}`,
    type: "application",
    identifier: application.id,
    name: application.name,
    active: application.active,
  };
  const nodes = [appNode, ...roles, ...capabilities, ...resources, ...scopes];
  const edges = [];
  const add = (source, relationship, target) =>
    edges.push({
      id: `edge:${source}:${relationship}:${target}`,
      source,
      target,
      relationship,
    });
  for (const node of roles) add(appNode.id, "application_role", node.id);
  for (const node of capabilities)
    add(appNode.id, "application_capability", node.id);
  for (const node of resources)
    add(appNode.id, "application_resource", node.id);
  for (const node of scopes) {
    const resource = resources.find(
      (candidate) => candidate.id === node.resource_id,
    );
    add(resource.id, "resource_scope", node.id);
  }
  for (let index = 0; index < config.roleCapabilities; index++) {
    add(
      roles[index % roles.length].id,
      "role_capability",
      capabilities[Math.floor(index / roles.length) % capabilities.length].id,
    );
    add(
      resources[index % resources.length].id,
      "resource_capability",
      capabilities[Math.floor(index / resources.length) % capabilities.length]
        .id,
    );
  }
  for (let index = 0; index < config.scopeCapabilities; index++) {
    add(
      scopes[index % scopes.length].id,
      "scope_capability",
      capabilities[Math.floor(index / scopes.length) % capabilities.length].id,
    );
  }
  assert.equal(nodes.length, config.expectedNodes);
  assert.equal(edges.length, config.expectedEdges);
  return {
    application: app,
    policy_revision: "42",
    complete: true,
    nodes,
    edges,
  };
}

async function verifyPolicyMap(page, origin, application) {
  const smallRenderStarted = performance.now();
  await page.goto(
    `${origin}/console/policies?application_id=${application.id}`,
  );
  await page.locator(".svelte-flow__node").first().waitFor();
  const smallRenderMs = performance.now() - smallRenderStarted;
  const language = page.getByRole("combobox", { name: /^(Language|Idioma)$/ });
  await language.selectOption("en");
  await page
    .getByRole("heading", { name: "Application policy map", exact: true })
    .waitFor();
  assert.equal(
    await page
      .locator('#relationship-filter option[value="role_capability"]')
      .textContent(),
    "Role grants capability",
  );
  await language.selectOption("es");
  await page
    .getByRole("heading", {
      name: "Mapa de políticas de la aplicación",
      exact: true,
    })
    .waitFor();

  const read = await call(
    page,
    `/api/admin/console/applications/${application.id}/policy-map`,
  );
  assert.equal(read.status, 200);
  assert.equal(read.cache, "no-store");
  assert.deepEqual(
    [...new Set(read.body.edges.map((edge) => edge.relationship))].sort(),
    [
      "application_capability",
      "application_resource",
      "application_role",
      "resource_capability",
      "resource_scope",
      "role_capability",
      "scope_capability",
    ].sort(),
  );
  const role = read.body.nodes.find((node) => node.type === "role");
  assert.ok(role);
  const apiBytes = Buffer.byteLength(JSON.stringify(read.body));
  const flow = page.locator(".policy-flow");
  await flow.waitFor();
  const roleNode = page
    .locator(".svelte-flow__node")
    .filter({ hasText: "Console reader" });
  await roleNode.waitFor();
  await roleNode.focus();
  await roleNode.press("Enter");
  await page.locator(".policy-inspector").getByText("Console reader").waitFor();
  assert.equal(
    await page.locator(".policy-management-link").getAttribute("href"),
    `/console/roles?application_id=${application.id}&item_kind=role&item_id=${role.identifier}`,
  );
  await page
    .getByRole("button", { name: "Enfocar relaciones directas", exact: true })
    .click();
  await page
    .getByRole("status")
    .filter({ hasText: "Mostrando las conexiones directas" })
    .waitFor();
  await page.getByRole("button", { name: "Mostrar todo", exact: true }).click();

  let policyMutations = 0;
  const observeMutation = (request) => {
    if (
      request.method() !== "GET" &&
      new URL(request.url()).pathname.startsWith("/api/admin/")
    )
      policyMutations++;
  };
  page.on("request", observeMutation);
  await page.locator("#relationship-filter").selectOption("role_capability");
  await page
    .getByRole("button", { name: "Tabla accesible", exact: true })
    .click();
  await page
    .getByRole("table")
    .getByText("El rol concede la capacidad", { exact: true })
    .waitFor();
  assert.equal(await page.locator(".policy-flow").count(), 0);
  await page.locator("#relationship-filter").selectOption("");
  await page.locator("#policy-search").fill("Console reader");
  await page
    .getByRole("table")
    .getByText("Console reader", { exact: true })
    .first()
    .waitFor();
  assert.equal(
    policyMutations,
    0,
    "policy-map interactions must remain read-only",
  );
  page.off("request", observeMutation);

  await page.getByRole("button", { name: "Grafo", exact: true }).click();
  await page.locator("#policy-search").fill("");
  await page.locator("#relationship-filter").selectOption("");
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.screenshot({
    path: resolve(".local/policy-map-desktop.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.ok(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
    "Policy map must fit a mobile viewport",
  );
  await page.screenshot({
    path: resolve(".local/policy-map-mobile.png"),
    fullPage: true,
  });

  const representative = policyFixture(read.body.application, {
    roles: 127,
    capabilities: 128,
    resources: 128,
    scopes: 128,
    roleCapabilities: 512,
    scopeCapabilities: 513,
    expectedNodes: 512,
    expectedEdges: 2048,
  });
  const large = policyFixture(read.body.application, {
    roles: 512,
    capabilities: 512,
    resources: 512,
    scopes: 511,
    roleCapabilities: 2048,
    scopeCapabilities: 2049,
    expectedNodes: 2048,
    expectedEdges: 8192,
  });
  let activeFixture = representative;
  await page.route(
    `**/api/admin/console/applications/${application.id}/policy-map`,
    (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        headers: { "cache-control": "private, no-store" },
        body: JSON.stringify(activeFixture),
      }),
  );
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.evaluate(() =>
    performance.mark("policy-map-representative-start"),
  );
  await page
    .getByRole("button", {
      name: "Volver a cargar la instantánea",
      exact: true,
    })
    .click();
  await page.waitForFunction(
    (count) =>
      document
        .querySelector(".policy-snapshot")
        ?.textContent?.includes(String(count)),
    representative.nodes.length,
  );
  await flow.waitFor();
  await page.locator(".svelte-flow__node").first().waitFor();
  const representativeMetrics = await page.evaluate(() => {
    performance.mark("policy-map-representative-ready");
    performance.measure(
      "policy-map-representative-render",
      "policy-map-representative-start",
      "policy-map-representative-ready",
    );
    return {
      renderMs: performance
        .getEntriesByName("policy-map-representative-render")
        .at(-1).duration,
      visibleNodes: document.querySelectorAll(".svelte-flow__node").length,
      heapBytes: performance.memory?.usedJSHeapSize ?? null,
    };
  });
  assert.ok(representativeMetrics.visibleNodes > 0);
  assert.ok(representativeMetrics.visibleNodes <= representative.nodes.length);
  const representativeBytes = Buffer.byteLength(JSON.stringify(representative));
  activeFixture = large;
  await page.evaluate(() => performance.mark("policy-map-large-start"));
  await page
    .getByRole("button", {
      name: "Volver a cargar la instantánea",
      exact: true,
    })
    .click();
  await page.waitForFunction(
    (count) =>
      document
        .querySelector(".policy-snapshot")
        ?.textContent?.includes(String(count)),
    large.nodes.length,
  );
  await page
    .getByRole("heading", {
      name: "Este grafo es demasiado grande para mostrarlo de forma interactiva",
      exact: true,
    })
    .waitFor();
  assert.equal(await page.locator(".policy-flow").count(), 0);
  const largeMetrics = await page.evaluate(() => {
    performance.mark("policy-map-large-ready");
    performance.measure(
      "policy-map-large-render",
      "policy-map-large-start",
      "policy-map-large-ready",
    );
    return {
      renderMs: performance.getEntriesByName("policy-map-large-render").at(-1)
        .duration,
      visibleNodes: document.querySelectorAll(".svelte-flow__node").length,
      heapBytes: performance.memory?.usedJSHeapSize ?? null,
    };
  });
  assert.equal(largeMetrics.visibleNodes, 0);
  const largeBytes = Buffer.byteLength(JSON.stringify(large));
  await page.screenshot({
    path: resolve(".local/policy-map-bounded-large.png"),
    fullPage: true,
  });
  await page
    .locator(".policy-graph-limit")
    .getByRole("button", { name: "Tabla accesible", exact: true })
    .click();
  const largeTableCaptions = await page
    .locator(".policy-table caption")
    .allTextContents();
  assert.deepEqual(
    largeTableCaptions.map((caption) => caption.replace(/\D/g, "")),
    ["2048", "8192"],
    "the bounded graph fallback must expose the complete snapshot in paginated tables",
  );
  const largeTableRows = await page.locator(".policy-table tbody tr").count();
  assert.equal(largeTableRows, 100);

  await page.unroute(
    `**/api/admin/console/applications/${application.id}/policy-map`,
  );
  await page.route(
    `**/api/admin/console/applications/${application.id}/policy-map`,
    (route) =>
      route.fulfill({
        status: 403,
        contentType: "application/json",
        body: "{}",
      }),
  );
  await page
    .getByRole("button", {
      name: "Volver a cargar la instantánea",
      exact: true,
    })
    .click();
  await page
    .getByRole("alert")
    .filter({ hasText: "no tiene autorización" })
    .waitFor();
  assert.equal(await page.locator(".policy-flow").count(), 0);
  await page.unroute(
    `**/api/admin/console/applications/${application.id}/policy-map`,
  );
  await page.route(
    `**/api/admin/console/applications/${application.id}/policy-map`,
    (route) =>
      route.fulfill({
        status: 401,
        contentType: "application/json",
        body: "{}",
      }),
  );
  await page.reload();
  await page
    .getByRole("alert")
    .filter({ hasText: "Tu sesión de administración terminó" })
    .waitFor();
  assert.equal(await page.locator(".policy-flow").count(), 0);
  await page.unroute(
    `**/api/admin/console/applications/${application.id}/policy-map`,
  );
  await page.route(
    `**/api/admin/console/applications/${application.id}/policy-map`,
    (route) => route.continue(),
  );
  await page.reload();
  await page
    .locator(".svelte-flow__node")
    .filter({ hasText: "Console reader" })
    .waitFor();
  await page.unroute(
    `**/api/admin/console/applications/${application.id}/policy-map`,
  );

  await page.goto(`${origin}/console/applications`);
  await create(page, "application", "Policy race portal");
  await page
    .getByRole("button", {
      name: "Seleccionar browser@example.com",
      exact: true,
    })
    .click();
  await page.getByRole("button", { name: "Crear", exact: true }).click();
  await saved(page);
  const raceApplications = await call(
    page,
    "/api/admin/catalog/applications?search=Policy%20race",
  );
  assert.equal(raceApplications.status, 200);
  const otherApplication = raceApplications.body.items.find(
    (item) => item.name === "Policy race portal",
  );
  assert.ok(otherApplication);

  let startDelayed;
  let releaseDelayed;
  let finishDelayed;
  const delayedStarted = new Promise((resolve) => (startDelayed = resolve));
  const delayedGate = new Promise((resolve) => (releaseDelayed = resolve));
  const delayedFinished = new Promise((resolve) => (finishDelayed = resolve));
  const delayedPath = `**/api/admin/console/applications/${application.id}/policy-map`;
  await page.route(delayedPath, async (route) => {
    startDelayed();
    await delayedGate;
    try {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        headers: { "cache-control": "private, no-store" },
        body: JSON.stringify(read.body),
      });
    } catch {
      // The navigation should cancel this obsolete request before the reply.
    }
    finishDelayed();
  });
  await page.goto(
    `${origin}/console/policies?application_id=${application.id}`,
  );
  await delayedStarted;
  await page.goto(
    `${origin}/console/policies?application_id=${otherApplication.id}`,
  );
  await page
    .locator(".policy-snapshot h2")
    .filter({ hasText: "Policy race portal" })
    .waitFor();
  releaseDelayed();
  await delayedFinished;
  assert.equal(
    await page.locator(".policy-snapshot h2").innerText(),
    "Policy race portal",
    "a delayed snapshot for another application must not replace the selected application",
  );
  await page.unroute(delayedPath);
  console.log(
    `Policy map evidence: small API ${apiBytes} JSON bytes in ${read.durationMs.toFixed(1)} ms; small graph ready in ${smallRenderMs.toFixed(1)} ms; representative ${representative.nodes.length} nodes/${representative.edges.length} edges, ${representativeBytes} JSON bytes, ${representativeMetrics.visibleNodes} DOM nodes, ${representativeMetrics.renderMs.toFixed(1)} ms, heap ${representativeMetrics.heapBytes ?? "unavailable"} bytes; bounded-large ${large.nodes.length} nodes/${large.edges.length} edges, ${largeBytes} JSON bytes, graph fallback in ${largeMetrics.renderMs.toFixed(1)} ms with ${largeMetrics.visibleNodes} graph DOM nodes, ${largeTableRows} paginated table rows, heap ${largeMetrics.heapBytes ?? "unavailable"} bytes.`,
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
  await verifyPolicyMap(page, origin, app);
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
