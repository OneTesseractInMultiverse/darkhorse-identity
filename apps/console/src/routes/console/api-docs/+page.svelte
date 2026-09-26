<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import ConsoleShell from '$lib/components/admin/ConsoleShell.svelte';
	import ApiReferencePanel from '$lib/components/admin/ApiReferencePanel.svelte';
</script>

<svelte:head>
	<title>{$language.t('console.pageTitle', { name: $language.t('console.apiDocs') })}</title>
	<meta name="description" content={$language.t('apiDocs.intro')} />
</svelte:head>

<ConsoleShell active="api-docs"><ApiReferencePanel /></ConsoleShell>
