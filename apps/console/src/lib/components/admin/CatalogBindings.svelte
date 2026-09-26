<script lang="ts">
	import { useCatalogLocalization } from '$lib/i18n/catalog-context';
	const catalog = useCatalogLocalization();
	import { Button } from '$lib/components/ui/button';
	import type { CatalogApi, View, Command } from '$lib/admin/catalog';
	import { binding } from '$lib/admin/catalog-changes';
	import CatalogPicker from './CatalogPicker.svelte';
	let {
		view,
		api,
		pending,
		save
	}: { view: View; api: CatalogApi; pending: boolean; save: (command: Command) => void } = $props();
	let choosing = $state<'applications' | 'capabilities' | null>(null),
		confirmation = $state<{
			command: Command;
			name: string;
			application: boolean;
			adding: boolean;
		} | null>(null);
	function propose(id: string, name: string, application: boolean, adding: boolean) {
		confirmation = {
			command: binding(view.item, id, application, adding),
			name,
			application,
			adding
		};
		choosing = null;
	}
	function confirm() {
		if (confirmation && !pending) {
			const command = confirmation.command;
			confirmation = null;
			save(command);
		}
	}
</script>

{#if view.item.kind === 'capability'}<p>{view.item.meaning}</p>
	<p class="muted">
		{$catalog.t('binding.meaningHelp')}
	</p>{/if}
{#if view.item.kind === 'resource'}<dl class="catalog-facts">
		<div class="catalog-fact-wide">
			<dt>{$catalog.t('binding.audience')}</dt>
			<dd class="catalog-id">{view.item.audience}</dd>
			<dd class="catalog-help">
				{$catalog.t('binding.audienceHelp')}
			</dd>
		</div>
	</dl>{/if}
{#if view.item.kind === 'role' || view.item.kind === 'capability'}
	<section class="catalog-section binding-section">
		<h3>{$catalog.t('binding.applications')}</h3>
		<p class="muted">
			{$catalog.t('binding.help')}
		</p>
		<ul class="catalog-options">
			{#each view.applications as a (a.id)}<li>
					<span>{a.name}</span><Button
						type="button"
						variant="outline"
						disabled={pending}
						onclick={() => propose(a.id, a.name, true, false)}
						aria-label={$catalog.t('binding.unbindName', { name: a.name })}
						>{$catalog.t('binding.unbind')}</Button
					>
				</li>{/each}
		</ul>
		<Button
			type="button"
			variant="outline"
			disabled={pending || view.item.active === false}
			onclick={() => (choosing = 'applications')}>{$catalog.t('binding.bind')}</Button
		>
	</section>
{/if}
{#if view.item.kind !== 'capability'}
	<section class="catalog-section binding-section">
		<h3>
			{view.item.kind === 'role'
				? $catalog.t('binding.grants')
				: view.item.kind === 'scope'
					? $catalog.t('binding.bounds')
					: $catalog.t('binding.exposed')}
		</h3>
		<p class="muted">
			{view.item.kind === 'role'
				? $catalog.t('binding.roleHelp')
				: view.item.kind === 'scope'
					? $catalog.t('binding.scopeHelp')
					: $catalog.t('binding.resourceHelp')}
		</p>
		<ul class="catalog-options">
			{#each view.capabilities as c (c.id)}<li>
					<span
						>{c.active === false ? $catalog.t('binding.retired', { name: c.name }) : c.name}</span
					><Button
						type="button"
						variant="outline"
						disabled={pending}
						onclick={() => propose(c.id, c.name, false, false)}
						aria-label={$catalog.t('binding.removeName', { name: c.name })}
						>{$catalog.t('remove')}</Button
					>
				</li>{/each}
		</ul>
		<Button
			type="button"
			variant="outline"
			disabled={pending}
			onclick={() => (choosing = 'capabilities')}>{$catalog.t('binding.add')}</Button
		>
	</section>
{/if}
{#if choosing}{@const selection = choosing}<CatalogPicker
		label={selection === 'applications'
			? $catalog.t('binding.chooseApplications')
			: $catalog.t('binding.chooseCapabilities')}
		load={(search, after) =>
			api.list(selection, {
				search,
				after,
				...(selection === 'capabilities'
					? { status: 'active', application_id: view.item.application_id }
					: {})
			})}
		choose={(item) => propose(item.id, item.name, selection === 'applications', true)}
		disabled={pending}
	/><Button type="button" variant="outline" disabled={pending} onclick={() => (choosing = null)}
		>{$catalog.t('binding.cancelSelection')}</Button
	>{/if}
{#if confirmation}<section class="catalog-confirm" aria-label={$catalog.t('binding.confirmTitle')}>
		<p>
			{$catalog.t(
				confirmation.application
					? confirmation.adding
						? 'binding.addApplication'
						: 'binding.removeApplication'
					: confirmation.adding
						? 'binding.addCapability'
						: 'binding.removeCapability',
				{ name: confirmation.name }
			)}
		</p>
		<p class="muted">
			{$catalog.t('binding.confirmHelp')}
		</p>
		<Button type="button" variant="outline" disabled={pending} onclick={() => (confirmation = null)}
			>{$catalog.t('binding.cancel')}</Button
		><Button type="button" disabled={pending} onclick={confirm}
			>{$catalog.t('binding.confirm')}</Button
		>
	</section>{/if}
