<script lang="ts">
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
		<label for="account-language">Preferred language</label>
		<select id="account-language" bind:value={selected} aria-describedby="account-language-help">
			<option value="">Automatic</option>
			<option value="en" lang="en">English</option>
			<option value="es" lang="es">Español</option>
		</select>
		<p id="account-language-help">
			Save a language for this account across browsers. Automatic uses your browser preferences and
			the deployment default. An explicit choice in the sign-in page takes precedence for that
			visit. The sign-in page and account overview currently support both languages.
		</p>
		<div class="modal-actions">
			<Button type="button" variant="outline" onclick={cancel}>Cancel</Button><Button type="submit"
				>{pending ? 'Saving…' : 'Save language'}</Button
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
