<script lang="ts">
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
	><title>Darkhorse — Your sessions</title><meta
		name="description"
		content="Review and manage your Darkhorse sign-in sessions"
	/></svelte:head
>
<div class="portal">
	<header class="topbar">
		<a href={resolve('/')} class="wordmark" aria-label="Darkhorse home"
			><img src={logo} alt="DarkHorse" width="336" height="64" /></a
		><span class="edition">ACCOUNT SECURITY <span class="version">/ SESSIONS</span></span>
	</header>
	<main id="main"><SessionsPanel {read} {end} /></main>
	<footer><span>DARKHORSE / IDENTITY SYSTEMS</span><span>DEVELOPMENT PREVIEW</span></footer>
</div>

<style>
	main {
		grid-template-columns: minmax(0, 1fr);
	}
</style>
