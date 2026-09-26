<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import { onMount, type Snippet } from 'svelte';
	let {
		title,
		pending = false,
		wide = false,
		description = '',
		close,
		children
	}: {
		title: string;
		pending?: boolean;
		wide?: boolean;
		description?: string;
		close: () => void;
		children: Snippet;
	} = $props();
	let dialog: HTMLDialogElement;
	onMount(() => {
		dialog.showModal();
	});
	function cancel(event: Event) {
		if (pending) event.preventDefault();
	}
</script>

<dialog
	bind:this={dialog}
	class="admin-modal"
	class:catalog-modal={wide}
	aria-describedby={description ? 'admin-modal-description' : undefined}
	aria-labelledby="admin-modal-title"
	oncancel={cancel}
	onclose={() => {
		if (!pending) close();
	}}
>
	<div class="modal-header">
		{#if wide}<button
				type="button"
				class="catalog-dialog-close"
				aria-label={$language.t('common.closeDialog')}
				disabled={pending}
				onclick={close}><span aria-hidden="true">×</span></button
			>{/if}
		<p class="eyebrow">{$language.t('common.console')}</p>
		<h2 id="admin-modal-title">{title}</h2>
		{#if description}<p id="admin-modal-description" class="catalog-help">{description}</p>{/if}
	</div>
	{@render children()}
</dialog>
