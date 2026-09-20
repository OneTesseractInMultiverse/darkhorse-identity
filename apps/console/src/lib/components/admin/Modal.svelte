<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	let {
		title,
		pending = false,
		close,
		children
	}: { title: string; pending?: boolean; close: () => void; children: Snippet } = $props();
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
	aria-labelledby="admin-modal-title"
	oncancel={cancel}
	onclose={() => {
		if (!pending) close();
	}}
>
	<div class="modal-header">
		<p class="eyebrow">MANAGEMENT CONSOLE</p>
		<h2 id="admin-modal-title">{title}</h2>
	</div>
	{@render children()}
</dialog>
