<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import ConsoleShell from '$lib/components/admin/ConsoleShell.svelte';
	import CatalogPanel from '$lib/components/admin/CatalogPanel.svelte';
	import { catalogApi } from '$lib/admin/catalog';
	const api = catalogApi((input, init) => fetch(input, init));
</script>

<svelte:head
	><title>{$language.t('console.pageTitle', { name: $language.t('console.applications') })}</title
	></svelte:head
>
<ConsoleShell active="applications"><CatalogPanel kind="applications" {api} /></ConsoleShell>
