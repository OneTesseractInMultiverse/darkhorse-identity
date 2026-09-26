<script lang="ts">
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { provideLocalization } from '$lib/i18n/context';
	import { browserStorage, readPreference } from '$lib/i18n/browser';
	import { localeList } from '$lib/i18n/input';
	import { presentation } from '$lib/i18n/presentation';
	let { children } = $props();
	const language = provideLocalization();
	let mounted = $state(false);
	onMount(() => {
		let alive = true;
		language.initialize({
			anonymous: readPreference(browserStorage()),
			browser: localeList(navigator.languages)
		});
		void presentation(fetch).then((locale) => {
			if (alive) language.deployment(locale);
		});
		mounted = true;
		return () => {
			alive = false;
		};
	});
	$effect(() => {
		if (mounted) {
			// Other routes remain English until their complete translation is delivered.
			document.documentElement.lang = page.url.pathname === '/' ? $language.locale : 'en';
			document.documentElement.dir = 'ltr';
		}
	});
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>
{@render children()}
