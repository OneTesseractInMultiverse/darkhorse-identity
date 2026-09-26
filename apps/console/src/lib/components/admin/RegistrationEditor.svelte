<script lang="ts">
	import { useCatalogLocalization } from '$lib/i18n/catalog-context';
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	const catalog = useCatalogLocalization();
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
	<section class="catalog-form-section">
		<h3>{$catalog.t('general')}</h3>
		<label class="catalog-required" for="catalog-name"
			>{$catalog.t(kind === 'capabilities' ? 'permissionKey' : 'name')}</label
		><input
			id="catalog-name"
			aria-describedby="catalog-name-help"
			bind:value={name}
			required
			maxlength={kind === 'capabilities' ? 200 : 100}
			disabled={pending}
			autocomplete="off"
		/>
		<p id="catalog-name-help" class="catalog-help">{$catalog.t(`nameHelp.${kind}`)}</p>
	</section>
	{#if kind === 'applications'}
		<section class="catalog-form-section">
			<h3>{$catalog.t('owner')}</h3>
			<p class="muted">
				{$catalog.t('ownerHelp')}
			</p>
			<p>{$catalog.t('ownerValue', { name: ownerName || $catalog.t('selectOwner') })}</p>
			<CatalogPicker
				label={$catalog.t('owner')}
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
				<label for="catalog-active">{$language.t('common.active')}</label>
				<p id="catalog-active-help" class="catalog-help">
					{kind === 'applications'
						? $catalog.t('activateApplication')
						: $catalog.t('activateClient')}
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
			<label class="catalog-required" for="capability-meaning">{$catalog.t('meaning')}</label
			><textarea
				id="capability-meaning"
				aria-describedby="capability-meaning-help"
				bind:value={meaning}
				required
				maxlength="1000"
				disabled={pending}></textarea>
			<p id="capability-meaning-help" class="catalog-help">
				{$catalog.t('meaningHelp')}
			</p>
		</section>{/if}
	{#if kind === 'roles' || kind === 'capabilities'}<p class="muted">
			{application ? $catalog.t('boundDefinition') : $catalog.t('sharedDefinition')}
		</p>{/if}
	{#if kind === 'scopes'}<section class="catalog-form-section">
			<h3>{$catalog.t('protectedResource')}</h3>
			<p class="catalog-help">
				{$catalog.t('resourceHelp')}
			</p>
			<p>{$catalog.t('resourceValue', { name: resourceName || $catalog.t('selectResource') })}</p>
			<CatalogPicker
				label={$catalog.t('scopeResource')}
				load={(search, after) =>
					api.list('resources', { application_id: application, search, after })}
				choose={(item) => {
					resource = item.id;
					resourceName = item.name;
				}}
				disabled={pending}
			/>
			<p class="muted">
				{$catalog.t('scopeHelp')}
			</p>
		</section>{/if}
	{#if kind === 'clients'}
		<section class="catalog-form-section">
			<h3>{$catalog.t('callbacksHeading')}</h3>
			<p class="catalog-help">
				{$catalog.t('clientHelp')}
			</p>
			<label class="catalog-required" for="client-callbacks">{$catalog.t('callbacks')}</label
			><textarea
				id="client-callbacks"
				aria-describedby="client-callbacks-help"
				bind:value={redirects}
				required
				maxlength="16391"
				rows="4"
				placeholder="https://app.example/callback"
				disabled={pending}></textarea>
			<p id="client-callbacks-help" class="catalog-help">
				{$catalog.t('callbacksHelp')}
			</p>
		</section>
		<section class="catalog-toggle">
			<div>
				<label for="client-refresh">{$catalog.t('allowRefresh')}</label>
				<p id="client-refresh-help" class="catalog-help">
					{$catalog.t('refreshHelp')}
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
			<h3>
				{$catalog.t('apiAccess')} <span class="catalog-optional">{$catalog.t('optional')}</span>
			</h3>
			<p class="catalog-help">
				{$catalog.t('allowancesHelp')}
			</p>

			<h4>{$catalog.t('allowedResources')}</h4>
			<p class="catalog-help">
				{$catalog.t('resourcesHelp')}
			</p>
			<ul class="catalog-options">
				{#each resources as r (r.id)}<li>
						<span>{r.name}</span><Button
							type="button"
							variant="outline"
							disabled={pending}
							onclick={() => (resources = resources.filter((v) => v.id !== r.id))}
							aria-label={$catalog.t('removeResource', { name: r.name })}
							>{$catalog.t('remove')}</Button
						>
					</li>{/each}
			</ul>
			<CatalogPicker
				label={$catalog.t('clientResources')}
				load={(search, after) =>
					api.list('resources', { application_id: application, search, after })}
				choose={(item) => (resources = add(resources, item))}
				disabled={pending || resources.length >= 32}
			/>
			<h4>{$catalog.t('allowedScopes')}</h4>
			<p class="catalog-help">
				{$catalog.t('scopesHelp')}
			</p>
			<ul class="catalog-options">
				{#each scopes as s (s.id)}<li>
						<span>{s.name}</span><Button
							type="button"
							variant="outline"
							disabled={pending}
							onclick={() => (scopes = scopes.filter((v) => v.id !== s.id))}
							aria-label={$catalog.t('removeScope', { name: s.name })}
							>{$catalog.t('remove')}</Button
						>
					</li>{/each}
			</ul>
			<CatalogPicker
				label={$catalog.t('clientScopes')}
				load={(search, after) => api.list('scopes', { application_id: application, search, after })}
				choose={(item) => (scopes = add(scopes, item))}
				disabled={pending || scopes.length >= 128}
			/>
			<p class="muted">
				{$catalog.t('allowancesCaution')}
			</p>
		</section>
	{/if}
	<div class="modal-actions">
		<Button type="button" variant="outline" disabled={pending} onclick={cancel}
			>{$language.t('common.cancel')}</Button
		><Button
			type="submit"
			disabled={pending || (kind === 'applications' && !owner) || (kind === 'scopes' && !resource)}
			>{$catalog.t(target ? 'saveChanges' : 'create')}</Button
		>
	</div>
</form>
