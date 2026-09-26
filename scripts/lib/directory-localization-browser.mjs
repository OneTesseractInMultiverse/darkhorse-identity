import assert from "node:assert/strict";
import { expect } from "@playwright/test";
import { resolve } from "node:path";

export async function verifyDirectoryPresentation(page, origin) {
  const writes = [];
  const record = (request) => {
    if (request.method() !== "GET") writes.push(request.method());
  };
  page.on("request", record);
  try {
    await page.goto(`${origin}/console/users`);
    const selector = page.getByRole("combobox", {
      name: /^(Language|Idioma)$/,
    });
    await selector.selectOption("es");
    await expect(page.locator("html")).toHaveAttribute("lang", "es");
    await page
      .getByRole("heading", { name: "Directorio de usuarios", exact: true })
      .waitFor();
    await page.getByLabel("Buscar usuarios").fill("directory@");
    await page.getByRole("button", { name: "Buscar", exact: true }).click();
    const view = page.getByRole("button", {
      name: "Ver a Directory Fixture",
      exact: true,
    });
    await view.click();
    await page
      .getByRole("button", { name: "Editar nombre", exact: true })
      .click();
    await page.getByLabel("Nombre", { exact: true }).fill("María 🦀");
    // A modal makes the header inert. Close/reopen preserves the original stored
    // record; changing a draft has not implicitly saved it.
    await page.keyboard.press("Escape");
    await expect(page.locator("dialog")).toHaveCount(0);
    await expect(view).toBeFocused();
    await selector.selectOption("en");
    await page
      .getByRole("button", { name: "View Directory Fixture", exact: true })
      .click();
    await page.getByRole("button", { name: "Edit name", exact: true }).click();
    await expect(page.getByLabel("First name")).toHaveValue("Directory");
    await page.getByRole("button", { name: "Cancel", exact: true }).click();
    await expect(page.locator("dialog")).toHaveCount(0);
    await selector.selectOption("es");
    await page.screenshot({
      path: resolve(".local/directory-es-desktop.png"),
      fullPage: true,
    });
    await page.setViewportSize({ width: 390, height: 844 });
    assert.ok(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    );
    await page
      .getByRole("button", {
        name: "Asignar acceso a Directory Fixture",
        exact: true,
      })
      .click();
    await page
      .getByRole("button", { name: "Guardar acceso", exact: true })
      .waitFor();
    await page.screenshot({
      path: resolve(".local/directory-es-mobile.png"),
      fullPage: true,
    });
    assert.ok(
      await page.evaluate(() => {
        const box = document.querySelector("dialog").getBoundingClientRect();
        return box.left >= 0 && box.right <= innerWidth;
      }),
    );
    await page.keyboard.press("Escape");
    await expect(page.locator("dialog")).toHaveCount(0);
    assert.deepEqual(
      writes,
      [],
      "Presentation, drafts and cancellation must not write",
    );
  } finally {
    page.off("request", record);
    await page.setViewportSize({ width: 1280, height: 900 });
  }
  console.log(
    "Directory bilingual presentation, draft cancellation, focus and mobile bounds passed.",
  );
}
