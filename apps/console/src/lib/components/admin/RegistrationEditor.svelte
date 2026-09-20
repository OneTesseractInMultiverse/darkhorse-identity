<script lang="ts">
	import { untrack } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import type { CatalogApi, Command, Item, Kind } from '$lib/admin/catalog';
	import { directoryApi } from '$lib/admin/directory';
	import CatalogPicker from './CatalogPicker.svelte';
	let {
		kind,
		application,
		target = null,
		api,
		pending,
		save,
		cancel
	}: {
		kind: Kind;
		application?: string;
		target?: Item | null;
		api: CatalogApi;
		pending: boolean;
		save: (command: Command) => void;
		cancel: () => void;
	} = $props();
	const initial = untrack(() => target);
	let name = $state(initial?.name ?? ''),
		active = $state(initial?.active ?? true),
		owner = $state(initial?.owner_id ?? ''),
		ownerName = $state(initial?.owner_email ?? ''),
		resource = $state(''),
		resourceName = $state(''),
		meaning = $state(''),
		redirects = $state(initial?.redirect_uris?.join('\n') ?? ''),
		refresh = $state(initial?.refresh_tokens ?? false);
	let resources = $state((initial?.resource_ids ?? []).map((id) => ({ id, name: id }))),
		scopes = $state((initial?.scope_ids ?? []).map((id) => ({ id, name: id })));
	const directory = directoryApi((input, init) => fetch(input, init));
	async function owners(search: string, after?: string) {
		const result = await directory.list({ search, status: 'active', after });
		return result.kind === 'ready'
			? {
					kind: 'ready' as const,
					data: {
						items: result.data.items.map((u) => ({ id: u.id, name: u.email })),
						next: result.data.next
					}
				}
			: result;
	}
	function command(): Command {
		if (kind === 'applications') {
			const application = { name: name.trim(), owner_id: owner, active };
			return target
				? {
						operation: 'update_application',
						application_id: target.id,
						revision: target.revision,
						application
					}
				: { operation: 'create_application', application };
		}
		if (kind === 'clients') {
			const client = {
				name: name.trim(),
				active,
				refresh_tokens: refresh,
				redirect_uris: redirects
					.split('\n')
					.map((s) => s.trim())
					.filter(Boolean),
				resource_ids: resources.map((r) => r.id),
				scope_ids: scopes.map((s) => s.id),
				token_endpoint_auth_method: 'client_secret_basic'
			};
			return target
				? {
						operation: 'update_client',
						application_id: application,
						client_id: target.id,
						revision: target.revision,
						client
					}
				: { operation: 'create_client', application_id: application, client };
		}
		if (kind === 'resources')
			return { operation: 'create_resource', application_id: application, name: name.trim() };
		if (kind === 'scopes')
			return {
				operation: 'create_scope',
				application_id: application,
				resource_id: resource,
				name: name.trim()
			};
		if (kind === 'roles')
			return { operation: 'create_role', name: name.trim(), application_id: application };
		return {
			operation: 'create_capability',
			key: name.trim(),
			meaning: meaning.trim(),
			application_id: application
		};
	}
	function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!pending) save(command());
	}
	function add(list: { id: string; name: string }[], item: { id: string; name: string }) {
		return list.some((v) => v.id === item.id) ? list : [...list, item];
	}
</script>

<form class="admin-form catalog-form" onsubmit={submit}>
	<label for="catalog-name">{kind === 'capabilities' ? 'Permission key' : 'Name'}</label><input
		id="catalog-name"
		bind:value={name}
		required
		maxlength={kind === 'capabilities' ? 200 : 100}
		disabled={pending}
		autocomplete="off"
	/>
	{#if kind === 'applications'}
		<p class="muted">
			The owner is an organizational contact. Ownership does not grant administration rights.
		</p>
		<p>Owner: <strong>{ownerName || 'Select an active user'}</strong></p>
		<CatalogPicker
			label="Application owner"
			load={owners}
			choose={(item) => {
				owner = item.id;
				ownerName = item.name;
			}}
			disabled={pending}
		/>
	{/if}
	{#if kind === 'applications' || kind === 'clients'}<label class="catalog-check"
			><input type="checkbox" bind:checked={active} disabled={pending} /> Active</label
		>{/if}
	{#if kind === 'capabilities'}<label for="capability-meaning">Meaning</label><textarea
			id="capability-meaning"
			bind:value={meaning}
			required
			maxlength="1000"
			disabled={pending}></textarea>
		<p class="muted">
			The key and meaning are permanent. Retire a capability when its meaning changes.
		</p>{/if}
	{#if kind === 'roles' || kind === 'capabilities'}<p class="muted">
			{application
				? 'The definition will be explicitly bound to this application.'
				: 'This shared definition grants no access until it is explicitly bound to an application.'}
		</p>{/if}
	{#if kind === 'scopes'}<p>Resource: <strong>{resourceName || 'Select a resource'}</strong></p>
		<CatalogPicker
			label="Scope resource"
			load={(search, after) =>
				api.list('resources', { application_id: application, search, after })}
			choose={(item) => {
				resource = item.id;
				resourceName = item.name;
			}}
			disabled={pending}
		/>
		<p class="muted">Scopes bound requested authority. They do not grant a user a role.</p>{/if}
	{#if kind === 'clients'}
		<label for="client-callbacks">Callback URLs</label><textarea
			id="client-callbacks"
			bind:value={redirects}
			required
			maxlength="16391"
			rows="4"
			placeholder="https://app.example/callback"
			disabled={pending}></textarea>
		<p class="muted">
			One exact HTTPS URL per line, up to eight. Authentication uses client_secret_basic.
		</p>
		<label class="catalog-check"
			><input type="checkbox" bind:checked={refresh} disabled={pending} /> Allow refresh tokens</label
		>
		<h3>Allowed resources</h3>
		<ul class="catalog-options">
			{#each resources as r (r.id)}<li>
					<span>{r.name}</span><Button
						type="button"
						variant="outline"
						disabled={pending}
						onclick={() => (resources = resources.filter((v) => v.id !== r.id))}
						aria-label={`Remove resource ${r.name}`}>Remove</Button
					>
				</li>{/each}
		</ul>
		<CatalogPicker
			label="Client resources"
			load={(search, after) =>
				api.list('resources', { application_id: application, search, after })}
			choose={(item) => (resources = add(resources, item))}
			disabled={pending || resources.length >= 32}
		/>
		<h3>Allowed scopes</h3>
		<ul class="catalog-options">
			{#each scopes as s (s.id)}<li>
					<span>{s.name}</span><Button
						type="button"
						variant="outline"
						disabled={pending}
						onclick={() => (scopes = scopes.filter((v) => v.id !== s.id))}
						aria-label={`Remove scope ${s.name}`}>Remove</Button
					>
				</li>{/each}
		</ul>
		<CatalogPicker
			label="Client scopes"
			load={(search, after) => api.list('scopes', { application_id: application, search, after })}
			choose={(item) => (scopes = add(scopes, item))}
			disabled={pending || scopes.length >= 128}
		/>
		<p class="muted">
			Each allowed scope must belong to an allowed resource. Existing permission limits remain
			enforced when policies change.
		</p>
	{/if}
	<div class="modal-actions">
		<Button type="button" variant="outline" disabled={pending} onclick={cancel}>Cancel</Button
		><Button
			type="submit"
			disabled={pending || (kind === 'applications' && !owner) || (kind === 'scopes' && !resource)}
			>{target ? 'Save changes' : 'Create'}</Button
		>
	</div>
</form>
