<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	import LanguageSelector from '$lib/components/LanguageSelector.svelte';
	const language = useLocalization();
	import { resolve } from '$app/paths';
	import logo from '$lib/assets/brand/logo.svg';
	import SessionsPanel from '$lib/components/SessionsPanel.svelte';
	import { readSessions, terminateSession } from '$lib/sessions';
	async function read(after?: string) {
		return readSessions(fetch, after);
	}
	async function end(id: string) {
		return terminateSession(fetch, id);
	}
</script>

<svelte:head
	><title>{$language.t('sessions.title')}</title><meta
		name="description"
		content={$language.t('sessions.description')}
	/></svelte:head
>
<div class="portal">
	<header class="topbar">
		<a href={resolve('/')} class="wordmark" aria-label={$language.t('portal.home')}
			><img src={logo} alt="DarkHorse" width="336" height="64" /></a
		><span class="edition"
			>{$language.t('sessions.security')}
			<span class="version">{$language.t('sessions.shortEdition')}</span></span
		>
		<LanguageSelector />
	</header>
	<main id="main"><SessionsPanel {read} {end} /></main>
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
