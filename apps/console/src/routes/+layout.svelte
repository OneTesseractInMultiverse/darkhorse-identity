<script lang="ts">
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { provideLocalization } from '$lib/i18n/context';
	import { browserStorage, readPreference } from '$lib/i18n/browser';
	import { localeList } from '$lib/i18n/input';
	import { resolveLocale } from '$lib/i18n/locale';
	let { children } = $props();
	const language = provideLocalization();
	let mounted = $state(false);
	onMount(() => {
		language.select(
			resolveLocale({
				anonymous: readPreference(browserStorage()),
				browser: localeList(navigator.languages)
			})
		);
		mounted = true;
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
