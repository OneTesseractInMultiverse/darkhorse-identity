<script lang="ts">
	import { untrack } from 'svelte';
	import { provideLocalization } from '../../../../src/lib/i18n/context';
	import { provideCatalogLocalization } from '../../../../src/lib/i18n/catalog-context';
	import type { Localization } from '../../../../src/lib/i18n/state';
	import type { Formatter } from '../../../../src/lib/i18n/format';
	import type { adminContract } from '../../../../src/lib/i18n/admin-contract';
	import type { CatalogApi, Kind } from '../../../../src/lib/admin/catalog';
	import CatalogPanel from '../../../../src/lib/components/admin/CatalogPanel.svelte';
	let {
		language,
		format,
		api,
		kind
	}: {
		language: Localization;
		format: Formatter<typeof adminContract>;
		api: CatalogApi;
		kind: Kind;
	} = $props();
	provideLocalization(untrack(() => language));
	provideCatalogLocalization(untrack(() => format));
</script>

<CatalogPanel {kind} {api} />
