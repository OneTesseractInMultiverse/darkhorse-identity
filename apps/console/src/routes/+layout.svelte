<script lang="ts">
	import './layout.css';
	import favicon from '#lib/assets/favicon.svg';
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { provideLocalization } from '#lib/i18n/context.ts';
	import { browserStorage, readPreference } from '#lib/i18n/browser.ts';
	import { localeList } from '#lib/i18n/input.ts';
	import { documentPresentation, presentation } from '#lib/i18n/presentation.ts';
	import { currentSession } from '#lib/authentication.ts';
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
		if (
			page.url.pathname.startsWith('/console/') ||
			['/security/sessions', '/security/keys', '/security/email', '/authorization'].includes(
				page.url.pathname
			)
		) {
			void language.restoreAccount(async () => {
				const result = await currentSession(fetch);
				return result.kind === 'signed-in' ? result.locale : undefined;
			});
		}
		mounted = true;
		return () => {
			alive = false;
		};
	});
	$effect(() => {
		if (mounted) {
			const metadata = documentPresentation(page.url.pathname, $language.locale);
			document.documentElement.lang = metadata[0];
			document.documentElement.dir = metadata[1];
		}
	});
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>
{@render children()}
