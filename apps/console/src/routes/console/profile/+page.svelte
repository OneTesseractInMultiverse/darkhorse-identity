<script lang="ts">
	import { useLocalization } from '#lib/i18n/context.ts';
	const language = useLocalization();
	import { browser } from '$app/env';
	import { page } from '$app/state';
	import ConsoleShell from '#lib/components/admin/ConsoleShell.svelte';
	import ProfilePanel from '#lib/components/ProfilePanel.svelte';
	import { mediaApi } from '#lib/media.ts';
	const images = mediaApi((input, init) => fetch(input, init));
	import { profileApi } from '#lib/profiles.ts';
	const api = profileApi((input, init) => fetch(input, init));
	const target = $derived(browser ? (page.url.searchParams.get('user') ?? 'invalid') : 'invalid');
</script>

<svelte:head><title>{$language.t('console.profileTitle')}</title></svelte:head>
<ConsoleShell
	>{#key target}<ProfilePanel {api} {images} {target} />{/key}</ConsoleShell
>
