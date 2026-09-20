<script lang="ts">
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
		confirmation = $state<{ command: Command; message: string } | null>(null);
	function propose(id: string, name: string, application: boolean, adding: boolean) {
		confirmation = {
			command: binding(view.item, id, application, adding),
			message: `${adding ? 'Add' : 'Remove'} ${name} ${application ? 'application binding' : 'capability'}?`
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
		Meaning is permanent. Retirement takes effect for new authorization checks.
	</p>{/if}
{#if view.item.kind === 'resource'}<p class="catalog-id">Audience: {view.item.audience}</p>{/if}
{#if view.item.kind === 'role' || view.item.kind === 'capability'}
	<h3>Application bindings</h3>
	<p class="muted">Each binding is explicit. New applications inherit no access.</p>
	<ul class="catalog-options">
		{#each view.applications as a (a.id)}<li>
				<span>{a.name}</span><Button
					type="button"
					variant="outline"
					disabled={pending}
					onclick={() => propose(a.id, a.name, true, false)}
					aria-label={`Unbind ${a.name}`}>Unbind</Button
				>
			</li>{/each}
	</ul>
	<Button
		type="button"
		variant="outline"
		disabled={pending || view.item.active === false}
		onclick={() => (choosing = 'applications')}>Bind application</Button
	>
{/if}
{#if view.item.kind !== 'capability'}
	<h3>
		{view.item.kind === 'role'
			? 'Granted capabilities'
			: view.item.kind === 'scope'
				? 'Capability bounds'
				: 'Exposed capabilities'}
	</h3>
	<p class="muted">
		{view.item.kind === 'role'
			? 'Grants require capability bindings in every application bound to this role.'
			: view.item.kind === 'scope'
				? 'A scope can include only capabilities exposed by its resource.'
				: 'Expose only capabilities explicitly bound to this application.'}
	</p>
	<ul class="catalog-options">
		{#each view.capabilities as c (c.id)}<li>
				<span>{c.name}{c.active === false ? ' (retired)' : ''}</span><Button
					type="button"
					variant="outline"
					disabled={pending}
					onclick={() => propose(c.id, c.name, false, false)}
					aria-label={`Remove ${c.name}`}>Remove</Button
				>
			</li>{/each}
	</ul>
	<Button
		type="button"
		variant="outline"
		disabled={pending}
		onclick={() => (choosing = 'capabilities')}>Add capability</Button
	>
{/if}
{#if choosing}{@const selection = choosing}<CatalogPicker
		label={selection === 'applications' ? 'Binding applications' : 'Binding capabilities'}
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
		>Cancel selection</Button
	>{/if}
{#if confirmation}<section class="catalog-confirm" aria-label="Confirm access change">
		<p>{confirmation.message}</p>
		<p class="muted">
			Changes affect new authorization checks immediately. Existing dependent bindings must be
			removed first.
		</p>
		<Button type="button" variant="outline" disabled={pending} onclick={() => (confirmation = null)}
			>Cancel change</Button
		><Button type="button" disabled={pending} onclick={confirm}>Confirm change</Button>
	</section>{/if}
