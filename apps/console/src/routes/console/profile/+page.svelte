<script lang="ts">
	import { browser } from '$app/environment';
	import { page } from '$app/state';
	import ConsoleShell from '$lib/components/admin/ConsoleShell.svelte';
	import ProfilePanel from '$lib/components/ProfilePanel.svelte';
	import { mediaApi } from '$lib/media';
	const images = mediaApi((input, init) => fetch(input, init));
	import { profileApi } from '$lib/profiles';
	const api = profileApi((input, init) => fetch(input, init));
	const target = $derived(browser ? (page.url.searchParams.get('user') ?? 'invalid') : 'invalid');
</script>

<svelte:head><title>User profile — Darkhorse</title></svelte:head>
<ConsoleShell
	>{#key target}<ProfilePanel {api} {images} {target} />{/key}</ConsoleShell
>
