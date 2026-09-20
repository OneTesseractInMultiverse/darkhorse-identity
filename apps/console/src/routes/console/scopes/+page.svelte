<script lang="ts">
	import ConsoleShell from '$lib/components/admin/ConsoleShell.svelte';
	import CatalogPanel from '$lib/components/admin/CatalogPanel.svelte';
	import { catalogApi } from '$lib/admin/catalog';
	const api = catalogApi((input, init) => fetch(input, init));
</script>

<svelte:head><title>Scopes — Darkhorse</title></svelte:head>
<ConsoleShell active="scopes"><CatalogPanel kind="scopes" {api} /></ConsoleShell>
