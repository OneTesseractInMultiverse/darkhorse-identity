<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	import LanguageSelector from '$lib/components/LanguageSelector.svelte';
	const language = useLocalization();
	import { resolve } from '$app/paths';
	import LoginBrand from '$lib/components/LoginBrand.svelte';
	import LoginPanel from '$lib/components/LoginPanel.svelte';
	import { authenticate, currentSession, endSession } from '$lib/authentication';
	async function signIn(email: string, password: string) {
		return authenticate(fetch, email, password);
	}
	async function checkSession() {
		return currentSession(fetch);
	}
	async function signOut() {
		return endSession(fetch);
	}
</script>

<svelte:head>
	<title>{$language.t('portal.title')}</title>
	<meta name="description" content={$language.t('portal.description')} />
</svelte:head>
<div class="portal" lang={$language.locale}>
	<header class="topbar">
		<a href={resolve('/')} class="wordmark" aria-label={$language.t('portal.home')}
			><LoginBrand fetcher={(input, init) => fetch(input, init)} /></a
		>
		<LanguageSelector />
		<span class="edition">{$language.t('portal.edition')} <span class="version">/ 0.1</span></span>
	</header>
	<main id="main">
		<LoginPanel {signIn} {checkSession} {signOut} showSecurityLink />
		<p class="caption">{$language.t('portal.caption')}</p>
	</main>
	<footer>
		<span>{$language.t('portal.footer')}</span><span>{$language.t('portal.preview')}</span>
	</footer>
</div>

<style>
	.portal {
		isolation: isolate;
	}
	.topbar {
		gap: 24px;
	}
	@media (max-width: 600px) {
		.topbar {
			flex-direction: column;
			align-items: flex-start;
			gap: 16px;
		}
	}
</style>
