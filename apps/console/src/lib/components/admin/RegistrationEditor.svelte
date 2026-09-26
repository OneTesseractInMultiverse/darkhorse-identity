<script lang="ts">
	import { untrack } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import type { CatalogApi, Command, Item, Kind } from '$lib/admin/catalog';
	import { directoryApi } from '$lib/admin/directory';
	import CatalogPicker from './CatalogPicker.svelte';
	import { nameHelp } from '$lib/admin/catalog-copy';
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
	<section class="catalog-form-section">
		<h3>General</h3>
		<label class="catalog-required" for="catalog-name"
			>{kind === 'capabilities' ? 'Permission key' : 'Name'}</label
		><input
			id="catalog-name"
			aria-describedby="catalog-name-help"
			bind:value={name}
			required
			maxlength={kind === 'capabilities' ? 200 : 100}
			disabled={pending}
			autocomplete="off"
		/>
		<p id="catalog-name-help" class="catalog-help">{nameHelp[kind]}</p>
	</section>
	{#if kind === 'applications'}
		<section class="catalog-form-section">
			<h3>Application owner</h3>
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
		</section>
	{/if}
	{#if kind === 'applications' || kind === 'clients'}
		<section class="catalog-toggle">
			<div>
				<label for="catalog-active">Active</label>
				<p id="catalog-active-help" class="catalog-help">
					{kind === 'applications'
						? 'Allow clients in this application to authenticate. Turning this off blocks authentication for all of its clients.'
						: 'Allow this client to authenticate while its parent application is also active. Turning this off blocks this client.'}
				</p>
			</div>
			<input
				id="catalog-active"
				type="checkbox"
				bind:checked={active}
				disabled={pending}
				aria-describedby="catalog-active-help"
			/>
		</section>
	{/if}
	{#if kind === 'capabilities'}<section class="catalog-form-section">
			<label class="catalog-required" for="capability-meaning">Meaning</label><textarea
				id="capability-meaning"
				aria-describedby="capability-meaning-help"
				bind:value={meaning}
				required
				maxlength="1000"
				disabled={pending}></textarea>
			<p id="capability-meaning-help" class="catalog-help">
				Describe exactly which action this permission permits. The key and meaning are permanent.
				Retire a capability when its meaning changes.
			</p>
		</section>{/if}
	{#if kind === 'roles' || kind === 'capabilities'}<p class="muted">
			{application
				? 'The definition will be explicitly bound to this application.'
				: 'This shared definition grants no access until it is explicitly bound to an application.'}
		</p>{/if}
	{#if kind === 'scopes'}<section class="catalog-form-section">
			<h3>Protected resource</h3>
			<p class="catalog-help">
				Required. Select the API this scope applies to. The resource cannot be reassigned after
				creation.
			</p>
			<p>Resource: <strong>{resourceName || 'Select a resource'}</strong></p>
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
			<p class="muted">
				After creating the scope, add capabilities already exposed by its resource. Requesting a
				scope does not assign a role to the user.
			</p>
		</section>{/if}
	{#if kind === 'clients'}
		<section class="catalog-form-section">
			<h3>Sign-in &amp; callbacks</h3>
			<p class="catalog-help">
				For web applications with a backend that can securely store a secret. Uses Authorization
				Code with PKCE (S256) and <code>client_secret_basic</code> over HTTPS.
			</p>
			<label class="catalog-required" for="client-callbacks">Callback URLs</label><textarea
				id="client-callbacks"
				aria-describedby="client-callbacks-help"
				bind:value={redirects}
				required
				maxlength="16391"
				rows="4"
				placeholder="https://app.example/callback"
				disabled={pending}></textarea>
			<p id="client-callbacks-help" class="catalog-help">
				Where Darkhorse sends the user after authorization. Enter one exact HTTPS URL per line, up
				to eight. Include the path; wildcards and fragments are not accepted. The URL in your
				application must match exactly, including any query string.
			</p>
		</section>
		<section class="catalog-toggle">
			<div>
				<label for="client-refresh">Allow refresh tokens</label>
				<p id="client-refresh-help" class="catalog-help">
					Let the backend renew access with rotating refresh tokens while the user’s session remains
					valid. Disabled by default. This does not enable offline access.
				</p>
			</div>
			<input
				id="client-refresh"
				type="checkbox"
				bind:checked={refresh}
				disabled={pending}
				aria-describedby="client-refresh-help"
			/>
		</section>
		<section class="catalog-form-section">
			<h3>Protected API access <span class="catalog-optional">Optional</span></h3>
			<p class="catalog-help">
				Leave these lists empty for identity-only sign-in with <code>openid</code>,
				<code>profile</code>
				and <code>email</code>. These built-in scopes do not need to be created here. Custom
				resource access needs separate allowances and user permissions.
			</p>

			<h4>Allowed resources</h4>
			<p class="catalog-help">
				The APIs this client may request tokens for. Select up to 32 resources from this
				application.
			</p>
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
			<h4>Allowed scopes</h4>
			<p class="catalog-help">
				The custom resource scopes this client may request. Select up to 128 scopes; each must
				belong to an allowed resource.
			</p>
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
				Allowing a resource or scope does not grant access to a user. Existing permission limits
				remain enforced when policies change.
			</p>
		</section>
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
