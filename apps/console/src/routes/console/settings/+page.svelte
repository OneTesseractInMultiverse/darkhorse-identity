<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import ConsoleShell from '$lib/components/admin/ConsoleShell.svelte';
	import BrandingPanel from '$lib/components/BrandingPanel.svelte';
	import { mediaApi } from '$lib/media';
	const api = mediaApi((input, init) => fetch(input, init));
</script>

<svelte:head><title>{$language.t('branding.title')}</title></svelte:head>
<ConsoleShell active="settings"><BrandingPanel {api} /></ConsoleShell>
