<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	import LanguageSelector from '$lib/components/LanguageSelector.svelte';
	const language = useLocalization();
	import { resolve } from '$app/paths';
	import logo from '$lib/assets/brand/logo.svg';
	import KeysPanel from '$lib/components/KeysPanel.svelte';
	import { keyApi } from '$lib/personal-keys';
	const api = keyApi((input, init) => fetch(input, init));
</script>

<svelte:head><title>{$language.t('keys.title')}</title></svelte:head>
<div class="portal">
	<header class="topbar">
		<a href={resolve('/')} class="wordmark" aria-label={$language.t('portal.home')}
			><img src={logo} alt="Darkhorse" width="336" height="64" /></a
		><span class="edition">{$language.t('keys.edition')}</span>
		<LanguageSelector />
	</header>
	<main id="main"><KeysPanel {api} /></main>
	<footer>
		<span>DARKHORSE / IDENTITY SYSTEMS</span><span>{$language.t('portal.preview')}</span>
	</footer>
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
