<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	import LanguageSelector from '$lib/components/LanguageSelector.svelte';
	const language = useLocalization();
	import { resolve } from '$app/paths';
	import { replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import logo from '$lib/assets/brand/logo.svg';
	import EmailVerificationPanel from '$lib/components/EmailVerificationPanel.svelte';
	import { fragmentToken, emailStatus, requestEmail, confirmEmail } from '$lib/email-verification';
	import { authenticate, currentSession, endSession } from '$lib/authentication';
	function takeToken() {
		const token = fragmentToken(window.location.hash);
		if (window.location.hash) replaceState(resolve('/security/email'), page.state);
		return token;
	}
	function watchToken(changed: () => void) {
		window.addEventListener('hashchange', changed);
		return () => window.removeEventListener('hashchange', changed);
	}
	async function read() {
		return emailStatus(fetch);
	}
	async function request() {
		return requestEmail(fetch);
	}
	async function confirm(token: string) {
		return confirmEmail(fetch, token);
	}
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

<svelte:head
	><title>{$language.t('email.title')}</title><meta
		name="referrer"
		content="no-referrer"
	/></svelte:head
>
<div class="portal">
	<header class="topbar">
		<a href={resolve('/')} class="wordmark" aria-label={$language.t('portal.home')}
			><img src={logo} alt="DarkHorse" width="336" height="64" /></a
		><span class="edition"
			>{$language.t('sessions.security')}
			<span class="version">{$language.t('email.shortEdition')}</span></span
		>
		<LanguageSelector />
	</header>
	<main id="main">
		<EmailVerificationPanel
			{read}
			{request}
			{confirm}
			{takeToken}
			{watchToken}
			{signIn}
			{checkSession}
			{signOut}
		/>
	</main>
	<footer>
		<span>DARKHORSE / IDENTITY SYSTEMS</span><span>{$language.t('portal.preview')}</span>
	</footer>
</div>

<style>
	main {
		grid-template-columns: minmax(0, 1fr);
		gap: 2rem;
	}
	@media (max-width: 900px) {
		.topbar {
			flex-wrap: wrap;
			gap: 1rem;
		}
	}
</style>
