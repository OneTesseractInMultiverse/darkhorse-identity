import { readFile } from "node:fs/promises";
import { compileCatalog } from "../apps/console/src/lib/i18n/catalog.ts";
import { contract } from "../apps/console/src/lib/i18n/contract.ts";
// Fixed source paths. Catalogs are parsed data, never imported executable templates.
for (const locale of ["en", "es"]) {
  const source = await readFile(
    new URL(
      `../apps/console/src/lib/i18n/catalogs/${locale}.json`,
      import.meta.url,
    ),
    "utf8",
  );
  if (Buffer.byteLength(source) > 256 * 1024)
    throw new Error("Catalog size limit exceeded.");
  compileCatalog(contract, JSON.parse(source), locale);
}
console.log(
  `English/Spanish catalogs passed: ${Object.keys(contract).length} typed messages each.`,
);
