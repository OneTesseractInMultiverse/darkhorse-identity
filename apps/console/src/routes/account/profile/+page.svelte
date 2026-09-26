<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	import LanguageSelector from '$lib/components/LanguageSelector.svelte';
	const language = useLocalization();
	import { resolve } from '$app/paths';
	import ProfilePanel from '$lib/components/ProfilePanel.svelte';
	import { mediaApi } from '$lib/media';
	const images = mediaApi((input, init) => fetch(input, init));
	import { profileApi } from '$lib/profiles';
	const api = profileApi((input, init) => fetch(input, init));
</script>

<svelte:head><title>{$language.t('profile.title')}</title></svelte:head>
<div class="portal">
	<header class="topbar">
		<a href={resolve('/')} class="wordmark">DARKHORSE</a><span class="edition"
			>{$language.t('profile.edition')}</span
		>
		<LanguageSelector />
	</header>
	<main><ProfilePanel {api} {images} /></main>
</div>

<style>
	main {
		grid-template-columns: minmax(0, 1fr);
	}
	@media (max-width: 900px) {
		.topbar {
			flex-wrap: wrap;
			gap: 1rem;
		}
	}
</style>
