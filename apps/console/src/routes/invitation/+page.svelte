<script lang="ts">
	import { onDestroy } from 'svelte';
	import { emailLink } from '$lib/i18n/email-link';
	let clearHint: (() => void) | undefined;
	onDestroy(() => clearHint?.());
	import { useLocalization } from '$lib/i18n/context';
	import LanguageSelector from '$lib/components/LanguageSelector.svelte';
	const language = useLocalization();
	import { resolve } from '$app/paths';
	import { replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import logo from '$lib/assets/brand/logo.svg';
	import InvitationPanel from '$lib/components/InvitationPanel.svelte';
	import { acceptInvitation, type InvitationInput } from '$lib/invitations';
	function takeToken() {
		const link = emailLink(window.location.hash, 'iv1');
		clearHint = language.hint(link?.locale);
		if (window.location.hash) replaceState(resolve('/invitation'), page.state);
		return link?.token;
	}
	function watchToken(changed: () => void) {
		window.addEventListener('hashchange', changed);
		return () => window.removeEventListener('hashchange', changed);
	}
	async function accept(input: InvitationInput) {
		return acceptInvitation(fetch, input);
	}
</script>

<svelte:head
	><title>{$language.t('invitation.title')}</title><meta
		name="referrer"
		content="no-referrer"
	/></svelte:head
>
<div class="portal">
	<header class="topbar">
		<a href={resolve('/')} class="wordmark" aria-label={$language.t('portal.home')}
			><img src={logo} alt="DarkHorse" width="336" height="64" /></a
		><span class="edition"
			>{$language.t('invitation.account')}
			<span class="version">{$language.t('invitation.edition')}</span></span
		>
		<LanguageSelector />
	</header>
	<main id="main"><InvitationPanel {accept} {takeToken} {watchToken} /></main>
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
