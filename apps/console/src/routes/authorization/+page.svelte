<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	import LanguageSelector from '$lib/components/LanguageSelector.svelte';
	const language = useLocalization();
	import { resolve } from '$app/paths';
	import logo from '$lib/assets/brand/logo.svg';
	import AuthorizationPanel from '$lib/components/AuthorizationPanel.svelte';
	import { loadAuthorization, decideAuthorization } from '$lib/authorization';
	import { authenticate } from '$lib/authentication';
	async function load() {
		return loadAuthorization(fetch);
	}
	async function decide(id: string, choice: 'approve' | 'deny') {
		return decideAuthorization(fetch, id, choice);
	}
	async function signIn(email: string, password: string) {
		return authenticate(fetch, email, password);
	}
	function navigate(url: string) {
		window.location.assign(url);
	}
</script>

<svelte:head
	><title>{$language.t('authorization.title')}</title><meta
		name="referrer"
		content="no-referrer"
	/></svelte:head
>
<div class="portal">
	<header class="topbar">
		<a href={resolve('/')} class="wordmark" aria-label={$language.t('portal.home')}
			><img src={logo} alt="DarkHorse" width="336" height="64" /></a
		><span class="edition">{$language.t('authorization.edition')}</span>
		<LanguageSelector />
	</header>
	<main id="main"><AuthorizationPanel {load} {decide} {signIn} {navigate} /></main>
	<footer><span>DARKHORSE / IDENTITY SYSTEMS</span></footer>
</div>

<style>
	@media (max-width: 900px) {
		.topbar {
			flex-wrap: wrap;
			gap: 1rem;
		}
	}
</style>
