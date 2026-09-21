<script lang="ts">
	import ConsoleShell from '$lib/components/admin/ConsoleShell.svelte';
	import BrandingPanel from '$lib/components/BrandingPanel.svelte';
	import { mediaApi } from '$lib/media';
	const api = mediaApi((input, init) => fetch(input, init));
</script>

<svelte:head><title>Login branding — Darkhorse</title></svelte:head>
<ConsoleShell active="settings"><BrandingPanel {api} /></ConsoleShell>
