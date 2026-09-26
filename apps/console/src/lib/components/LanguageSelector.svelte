<script lang="ts">
	import { onMount } from 'svelte';
	let ready = $state(false);
	onMount(() => {
		ready = true;
	});
	import { useLocalization } from '$lib/i18n/context';
	import { browserStorage, writePreference } from '$lib/i18n/browser';
	import type { Locale } from '$lib/i18n/locale';
	let {
		remember = (locale: Locale) => writePreference(browserStorage(), locale)
	}: { remember?: (locale: Locale) => boolean } = $props();
	import { localeTag } from '$lib/i18n/input';
	const language = useLocalization();
	let unsaved = $state(false);
	function change(event: Event) {
		const locale = localeTag((event.currentTarget as HTMLSelectElement).value);
		if (!locale) return;
		language.select(locale);
		unsaved = !remember(locale);
	}
</script>

<div class="language-selector" lang={$language.locale}>
	<label for="language">{$language.t('language.label')}</label>
	<select
		disabled={!ready}
		id="language"
		value={$language.locale}
		onchange={change}
		aria-describedby="language-help"
	>
		<option value="en" lang="en">English</option>
		<option value="es" lang="es">Español</option>
	</select>
	<p id="language-help">{$language.t('language.help')}</p>
	{#if unsaved}<p role="status">{$language.t('language.unsaved')}</p>{/if}
</div>

<style>
	.language-selector {
		font-size: 12px;
		max-width: 25rem;
	}
	label {
		margin-inline-end: 0.7rem;
		color: var(--color-muted-foreground);
	}
	select {
		min-height: 40px;
		padding: 0.4rem 0.7rem;
		background: #102018;
		color: var(--color-foreground);
		border: 1px solid var(--console-line);
		border-radius: 8px;
	}
	select:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 3px;
	}
	p {
		margin-top: 0.4rem;
		color: var(--color-muted-foreground);
		line-height: 1.5;
	}
</style>
