<script lang="ts">
	import { useCatalogLocalization } from '$lib/i18n/catalog-context';
	const catalog = useCatalogLocalization();
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import type { Read } from '$lib/admin/directory';
	let {
		label,
		load,
		choose,
		disabled = false
	}: {
		label: string;
		load: (
			search: string,
			after?: string
		) => Promise<Read<{ items: { id: string; name: string }[]; next: string | null }>>;
		choose: (item: { id: string; name: string }) => void;
		disabled?: boolean;
	} = $props();
	let search = $state(''),
		items = $state<{ id: string; name: string }[]>([]),
		next = $state<string | null>(null),
		pending = $state(false),
		error = $state(false);
	let mounted = false;
	onMount(() => {
		mounted = true;
		void refresh();
		return () => {
			mounted = false;
		};
	});
	async function refresh(after?: string) {
		pending = true;
		error = false;
		items = [];
		const result = await load(search.trim(), after);
		if (!mounted) return;
		if (result.kind === 'ready') {
			items = result.data.items;
			next = result.data.next;
		} else {
			next = null;
			error = true;
		}
		pending = false;
	}
</script>

<fieldset class="catalog-picker" disabled={disabled || pending}>
	<legend>{label}</legend>
	<div class="catalog-search">
		<input
			aria-label={$catalog.t('picker.searchLabel', { label })}
			bind:value={search}
			maxlength="100"
			autocomplete="off"
		/><Button type="button" variant="outline" onclick={() => refresh()}
			>{$catalog.t('picker.search')}</Button
		>
	</div>
	{#if error}<p role="alert" class="admin-error">{$catalog.t('picker.error')}</p>{/if}
	{#if pending}<p role="status">{$catalog.t('picker.loading')}</p>{:else if !items.length}<p
			class="muted"
		>
			{$catalog.t('picker.empty')}
		</p>{/if}
	<ul class="catalog-options">
		{#each items as item (item.id)}<li>
				<span>{item.name}</span><Button
					type="button"
					variant="outline"
					onclick={() => choose(item)}
					aria-label={$catalog.t('picker.selectName', { name: item.name })}
					>{$catalog.t('picker.select')}</Button
				>
			</li>{/each}
	</ul>
	{#if next}<Button type="button" variant="outline" onclick={() => refresh(next!)}
			>{$catalog.t('picker.more')}</Button
		>{/if}
</fieldset>
