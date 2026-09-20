<script lang="ts">
	import { resolve } from '$app/paths';
	import logo from '$lib/assets/brand/logo.svg';
	import KeysPanel from '$lib/components/KeysPanel.svelte';
	import { keyApi } from '$lib/personal-keys';
	const api = keyApi((input, init) => fetch(input, init));
</script>

<svelte:head><title>Personal API keys — Darkhorse</title></svelte:head>
<div class="portal">
	<header class="topbar">
		<a href={resolve('/')} class="wordmark" aria-label="Darkhorse home"
			><img src={logo} alt="Darkhorse" width="336" height="64" /></a
		><span class="edition">ACCOUNT SECURITY / API KEYS</span>
	</header>
	<main id="main"><KeysPanel {api} /></main>
	<footer><span>DARKHORSE / IDENTITY SYSTEMS</span><span>DEVELOPMENT PREVIEW</span></footer>
</div>

<style>
	main {
		grid-template-columns: minmax(0, 1fr);
	}
</style>
