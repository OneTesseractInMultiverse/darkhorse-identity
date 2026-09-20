<script lang="ts">
	import ConsoleShell from '$lib/components/admin/ConsoleShell.svelte';
	import DirectoryPanel from '$lib/components/admin/DirectoryPanel.svelte';
	import { directoryApi } from '$lib/admin/directory';
	const api = directoryApi((input, init) => fetch(input, init));
</script>

<svelte:head
	><title>User directory — Darkhorse</title><meta
		name="description"
		content="Manage your organization’s user directory"
	/></svelte:head
>
<ConsoleShell><DirectoryPanel {api} /></ConsoleShell>
