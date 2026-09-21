<script lang="ts">
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
	<title>Darkhorse — Sign in</title>
	<meta name="description" content="Sign in to your organization’s Darkhorse workspace" />
</svelte:head>
<div class="portal">
	<header class="topbar">
		<a href={resolve('/')} class="wordmark" aria-label="Darkhorse home"
			><LoginBrand fetcher={(input, init) => fetch(input, init)} /></a
		>
		<span class="edition">IDENTITY CONSOLE <span class="version">/ 0.1</span></span>
	</header>
	<main id="main">
		<LoginPanel {signIn} {checkSession} {signOut} showSecurityLink />
		<p class="caption">A CLEAR VIEW. A CONTROLLED PATH.</p>
	</main>
	<footer><span>DARKHORSE / IDENTITY SYSTEMS</span><span>DEVELOPMENT PREVIEW</span></footer>
</div>

<style>
	.portal {
		isolation: isolate;
	}
</style>
