<script lang="ts">
	import { resolve } from '$app/paths';
	import ProfilePanel from '$lib/components/ProfilePanel.svelte';
	import { mediaApi } from '$lib/media';
	const images = mediaApi((input, init) => fetch(input, init));
	import { profileApi } from '$lib/profiles';
	const api = profileApi((input, init) => fetch(input, init));
</script>

<svelte:head><title>My profile — Darkhorse</title></svelte:head>
<div class="portal">
	<header class="topbar">
		<a href={resolve('/')} class="wordmark">DARKHORSE</a><span class="edition"
			>MY ACCOUNT / PROFILE</span
		>
	</header>
	<main><ProfilePanel {api} {images} /></main>
</div>

<style>
	main {
		grid-template-columns: minmax(0, 1fr);
	}
</style>
