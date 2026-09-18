<script lang="ts">
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
	><title>Darkhorse — Connect application</title><meta
		name="referrer"
		content="no-referrer"
	/></svelte:head
>
<div class="portal">
	<header class="topbar">
		<a href={resolve('/')} class="wordmark" aria-label="Darkhorse home"
			><img src={logo} alt="DarkHorse" width="336" height="64" /></a
		><span class="edition">SECURE CONNECTION</span>
	</header>
	<main id="main"><AuthorizationPanel {load} {decide} {signIn} {navigate} /></main>
	<footer><span>DARKHORSE / IDENTITY SYSTEMS</span></footer>
</div>
