<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import { untrack } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import type { Locale } from '$lib/i18n/locale';
	let {
		locale,
		pending,
		save,
		cancel
	}: {
		locale: Locale | null;
		pending: boolean;
		save: (locale: Locale | null) => void;
		cancel: () => void;
	} = $props();
	let selected = $state(untrack(() => locale ?? ''));
</script>

<form
	onsubmit={(event) => {
		event.preventDefault();
		save(selected === 'en' || selected === 'es' ? selected : null);
	}}
>
	<fieldset disabled={pending}>
		<label for="account-language">{$language.t('profile.language')}</label>
		<select id="account-language" bind:value={selected} aria-describedby="account-language-help">
			<option value="">{$language.t('profile.automatic')}</option>
			<option value="en" lang="en">English</option>
			<option value="es" lang="es">Español</option>
		</select>
		<p id="account-language-help">
			{$language.t('profile.languageHelp')}
		</p>
		<div class="modal-actions">
			<Button type="button" variant="outline" onclick={cancel}
				>{$language.t('common.cancel')}</Button
			><Button type="submit"
				>{$language.t(pending ? 'common.saving' : 'profile.saveLanguage')}</Button
			>
		</div>
	</fieldset>
</form>

<style>
	fieldset {
		border: 0;
		padding: 0;
		min-width: 0;
	}
	label {
		display: block;
		margin-bottom: 8px;
	}
	select {
		padding: 12px;
		width: 100%;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: #08140f;
		color: var(--color-foreground);
	}
	p {
		margin-block: 16px;
		line-height: 1.6;
		color: var(--color-muted-foreground);
	}
</style>
