<script lang="ts">
	import { resolve } from '$app/paths';
	import logo from '$lib/assets/brand/logo.svg';
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
			><img src={logo} alt="DarkHorse" width="336" height="64" /></a
		>
		<span class="edition">IDENTITY CONSOLE <span class="version">/ 0.1</span></span>
	</header>
	<main id="main">
		<LoginPanel {signIn} {checkSession} {signOut} />
		<p class="caption">A CLEAR VIEW. A CONTROLLED PATH.</p>
	</main>
	<footer><span>DARKHORSE / IDENTITY SYSTEMS</span><span>DEVELOPMENT PREVIEW</span></footer>
</div>
