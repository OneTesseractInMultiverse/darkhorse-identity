<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import { type Settings, type MediaApi, type MediaFailure } from '$lib/media';
	import Modal from './admin/Modal.svelte';
	import ImageControl from './ImageControl.svelte';
	let { api }: { api: MediaApi } = $props();
	let settings = $state<Settings | null>(null),
		pending = $state(false),
		error = $state<MediaFailure | null>(null),
		selected = $state<'logo' | 'background' | null>(null);
	let alive = true;
	async function load() {
		pending = true;
		selected = null;
		error = null;
		const result = await api.settings();
		if (!alive) return;
		if (result.kind === 'ready') settings = result.value;
		else {
			settings = null;
			error = result.code;
		}
		pending = false;
	}
	onMount(() => {
		void load();
		return () => {
			alive = false;
			settings = null;
		};
	});
</script>

<section class="glass branding-panel">
	<p class="eyebrow">SYSTEM / APPEARANCE</p>
	<h1>Login branding</h1>
	<p>These images are visible to everyone visiting the sign-in page.</p>
	{#if error}<p role="alert">{$language.t(`media.error.${error}`)}</p>{/if}{#if pending}<p
			role="status"
		>
			Loading…
		</p>{/if}
	{#if settings}<p>
			Image storage: {settings.storage_enabled ? `enabled (${settings.bucket})` : 'disabled'}.
			Storage connection details are managed through deployment settings.
		</p>
		<div class="branding-grid">
			{#each ['logo', 'background'] as kind (kind)}{@const name = kind as 'logo' | 'background'}
				<section>
					<h2>{name === 'logo' ? 'Login logo' : 'Login background'}</h2>
					{#key settings.revision}{#if settings[name]}<img
								src={`/api/branding/${name}`}
								alt={name === 'logo' ? 'Current login logo' : 'Current login background'}
							/>{:else}<p>Using the bundled default.</p>{/if}{/key}<Button
						disabled={pending}
						onclick={() => {
							selected = name;
						}}>Change {name}</Button
					>
				</section>{/each}
		</div>{/if}
	<Button variant="outline" disabled={pending} onclick={load}>Reload settings</Button>
	{#if selected && settings}<Modal
			title={`Change login ${selected}`}
			{pending}
			close={() => {
				selected = null;
			}}
			><ImageControl
				cancel={() => {
					selected = null;
				}}
				{api}
				path={`/api/admin/branding/${selected}`}
				revision={settings.revision}
				changed={() => {
					void load();
				}}
				busy={(value) => {
					pending = value;
				}}
			/></Modal
		>{/if}
</section>

<style>
	.branding-panel {
		padding: 2rem;
		width: 100%;
	}
	.branding-grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 2rem;
		margin: 2rem 0;
	}
	.branding-grid img {
		display: block;
		max-width: 100%;
		max-height: 200px;
		object-fit: contain;
		margin-bottom: 1rem;
	}
	h1 {
		font-size: 2rem;
	}
	@media (max-width: 650px) {
		.branding-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
