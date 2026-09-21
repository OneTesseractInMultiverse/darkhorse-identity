<script lang="ts">
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import { validImage, type MediaApi } from '$lib/media';
	let {
		api,
		path,
		revision,
		changed,
		busy,
		cancel
	}: {
		api: MediaApi;
		path: string;
		revision: string;
		changed: () => void;
		busy: (value: boolean) => void;
		cancel: () => void;
	} = $props();
	let file = $state<File | null>(null),
		pending = $state(false),
		blocked = $state(false),
		error = $state('');
	let alive = true;
	async function submit(remove: boolean) {
		if (pending || blocked || (!remove && !file)) return;
		pending = true;
		busy(true);
		error = '';
		const result = remove
			? await api.remove(path, revision)
			: await api.upload(path, revision, file!);
		if (!alive) return;
		pending = false;
		busy(false);
		file = null;
		if (result.kind === 'ready') changed();
		else {
			error = result.message;
			blocked = true;
		}
	}
	function select(files: FileList | null) {
		const value = files?.[0];
		file = value && validImage(value) ? value : null;
		error = value && !file ? 'Choose a PNG or JPEG image up to 4 MiB.' : '';
	}
	onMount(() => () => {
		alive = false;
		file = null;
	});
</script>

<p>
	PNG or JPEG, up to 4 MiB and 2,048 pixels per side. Image metadata is removed. Replacing or
	removing the image takes effect immediately.
</p>
{#if error}<p role="alert">{error}</p>{/if}
<label
	>Choose image<input
		type="file"
		accept="image/png,image/jpeg"
		disabled={pending || blocked}
		onchange={(e) => select(e.currentTarget.files)}
	/></label
>
<div class="modal-actions">
	<Button variant="outline" disabled={pending} onclick={cancel}>Close</Button>
	<Button variant="outline" disabled={pending || blocked} onclick={() => submit(true)}
		>Remove image</Button
	><Button disabled={pending || blocked || !file} onclick={() => submit(false)}
		>{pending ? 'Saving…' : 'Upload image'}</Button
	>
</div>

<style>
	label {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	input {
		max-width: 100%;
		margin-bottom: 1rem;
	}
</style>
