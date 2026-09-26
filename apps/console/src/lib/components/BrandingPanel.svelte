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
	<p class="eyebrow">{$language.t('branding.eyebrow')}</p>
	<h1>{$language.t('branding.heading')}</h1>
	<p>{$language.t('branding.intro')}</p>
	{#if error}<p role="alert">{$language.t(`media.error.${error}`)}</p>{/if}{#if pending}<p
			role="status"
		>
			{$language.t('common.loading')}
		</p>{/if}
	{#if settings}<p>
			{settings.storage_enabled
				? $language.t('branding.storageEnabled', {
						bucket: settings.bucket ?? $language.t('common.unspecified')
					})
				: $language.t('branding.storageDisabled')}
		</p>
		<div class="branding-grid">
			{#each ['logo', 'background'] as kind (kind)}{@const name = kind as 'logo' | 'background'}
				<section>
					<h2>{$language.t(name === 'logo' ? 'branding.logo' : 'branding.background')}</h2>
					{#key settings.revision}{#if settings[name]}<img
								src={`/api/branding/${name}`}
								alt={$language.t(name === 'logo' ? 'branding.logoAlt' : 'branding.backgroundAlt')}
							/>{:else}<p>{$language.t('branding.default')}</p>{/if}{/key}<Button
						disabled={pending}
						onclick={() => {
							selected = name;
						}}
						>{$language.t(
							name === 'logo' ? 'branding.changeLogo' : 'branding.changeBackground'
						)}</Button
					>
				</section>{/each}
		</div>{/if}
	<Button variant="outline" disabled={pending} onclick={load}
		>{$language.t('branding.reload')}</Button
	>
	{#if selected && settings}<Modal
			title={$language.t(
				selected === 'logo' ? 'branding.changeLogoTitle' : 'branding.changeBackgroundTitle'
			)}
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
