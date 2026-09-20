<script lang="ts">
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
		error = $state('');
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
		error = '';
		items = [];
		const result = await load(search.trim(), after);
		if (!mounted) return;
		if (result.kind === 'ready') {
			items = result.data.items;
			next = result.data.next;
		} else {
			next = null;
			error = 'Options could not be loaded. Refresh or sign in again.';
		}
		pending = false;
	}
</script>

<fieldset class="catalog-picker" disabled={disabled || pending}>
	<legend>{label}</legend>
	<div class="catalog-search">
		<input
			aria-label={`Search ${label}`}
			bind:value={search}
			maxlength="100"
			autocomplete="off"
		/><Button type="button" variant="outline" onclick={() => refresh()}>Search options</Button>
	</div>
	{#if error}<p role="alert" class="admin-error">{error}</p>{/if}
	{#if pending}<p role="status">Loading options…</p>{:else if !items.length}<p class="muted">
			No matching options.
		</p>{/if}
	<ul class="catalog-options">
		{#each items as item (item.id)}<li>
				<span>{item.name}</span><Button
					type="button"
					variant="outline"
					onclick={() => choose(item)}
					aria-label={`Select ${item.name}`}>Select</Button
				>
			</li>{/each}
	</ul>
	{#if next}<Button type="button" variant="outline" onclick={() => refresh(next!)}
			>More options</Button
		>{/if}
</fieldset>
