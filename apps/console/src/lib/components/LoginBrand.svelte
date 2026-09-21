<script lang="ts">
	import { onMount } from 'svelte';
	import logo from '$lib/assets/brand/logo.svg';
	import { branding } from '$lib/media';
	let { fetcher }: { fetcher: typeof fetch } = $props();
	let custom = $state(false),
		background = $state(false);
	let alive = true;
	onMount(() => {
		void branding(fetcher).then((value) => {
			if (alive) {
				custom = value.logo;
				background = value.background;
			}
		});
		return () => {
			alive = false;
		};
	});
</script>

<img
	src={custom ? '/api/branding/logo' : logo}
	alt="DarkHorse"
	width="336"
	height="64"
	onerror={() => {
		custom = false;
	}}
/>
{#if background}<div class="login-background" aria-hidden="true">
		<img
			src="/api/branding/background"
			alt=""
			onerror={() => {
				background = false;
			}}
		/>
	</div>{/if}

<style>
	.login-background {
		position: fixed;
		inset: 0;
		z-index: -1;
		pointer-events: none;
		background: #06100b;
	}
	.login-background img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		opacity: 0.2;
		filter: brightness(0.6);
	}
	img {
		object-fit: contain;
	}
</style>
