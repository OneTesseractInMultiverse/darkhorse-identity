<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import { validImage, type MediaApi, type MediaFailure } from '$lib/media';
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
		error = $state<MediaFailure | null>(null);
	let alive = true;
	async function submit(remove: boolean) {
		if (pending || blocked || (!remove && !file)) return;
		pending = true;
		busy(true);
		error = null;
		const result = remove
			? await api.remove(path, revision)
			: await api.upload(path, revision, file!);
		if (!alive) return;
		pending = false;
		busy(false);
		file = null;
		if (result.kind === 'ready') changed();
		else {
			error = result.code;
			blocked = true;
		}
	}
	function select(files: FileList | null) {
		const value = files?.[0];
		file = value && validImage(value) ? value : null;
		error = value && !file ? 'file' : null;
	}
	onMount(() => () => {
		alive = false;
		file = null;
	});
</script>

<p>
	{$language.t('media.help')}
</p>
{#if error}<p role="alert">{$language.t(`media.error.${error}`)}</p>{/if}
<label
	>{$language.t('media.choose')}<input
		type="file"
		accept="image/png,image/jpeg"
		disabled={pending || blocked}
		onchange={(e) => select(e.currentTarget.files)}
	/></label
>
<div class="modal-actions">
	<Button variant="outline" disabled={pending} onclick={cancel}
		>{$language.t('common.close')}</Button
	>
	<Button variant="outline" disabled={pending || blocked} onclick={() => submit(true)}
		>{$language.t('media.remove')}</Button
	><Button disabled={pending || blocked || !file} onclick={() => submit(false)}
		>{$language.t(pending ? 'common.saving' : 'media.upload')}</Button
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
