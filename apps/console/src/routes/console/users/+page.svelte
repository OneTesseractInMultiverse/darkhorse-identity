<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import ConsoleShell from '$lib/components/admin/ConsoleShell.svelte';
	import DirectoryPanel from '$lib/components/admin/DirectoryPanel.svelte';
	import { directoryApi } from '$lib/admin/directory';
	const api = directoryApi((input, init) => fetch(input, init));
</script>

<svelte:head
	><title>{$language.t('directory.title')}</title><meta
		name="description"
		content={$language.t('directory.description')}
	/></svelte:head
>
<ConsoleShell><DirectoryPanel {api} /></ConsoleShell>
