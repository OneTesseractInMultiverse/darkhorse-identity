<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import {
		failureMessage,
		titles,
		type CatalogApi,
		type Kind,
		type Item,
		type View,
		type Page,
		type Query,
		type Command,
		type Failure,
		type Registered,
		type Write
	} from '$lib/admin/catalog';
	import { reference } from '$lib/admin/catalog-decode';
	import Modal from './Modal.svelte';
	import CatalogPicker from './CatalogPicker.svelte';
	import RegistrationEditor from './RegistrationEditor.svelte';
	import CatalogBindings from './CatalogBindings.svelte';
	import ClientCredentials from './ClientCredentials.svelte';
	let { kind, api }: { kind: Kind; api: CatalogApi } = $props();
	let page = $state<Page | null>(null),
		pending = $state(true),
		blocked = $state(false),
		error = $state(''),
		message = $state(''),
		search = $state(''),
		status = $state(''),
		application = $state<string | undefined>(),
		applicationName = $state('');
	let query = $state<Query>({}),
		cursors = $state<(string | undefined)[]>([undefined]),
		position = $state(0),
		selectApplication = $state(false);
	let target = $state<Item | null>(null),
		view = $state<View | null>(null),
		mode = $state<'create' | 'edit' | 'detail' | null>(null),
		reveal = $state<Registered | null>(null),
		retiring = $state(false);
	let mounted = false,
		trigger: HTMLElement | null = null;
	let notification = $state<HTMLParagraphElement>();
	const scoped = $derived(kind === 'clients' || kind === 'resources' || kind === 'scopes');
	onMount(() => {
		mounted = true;
		const selected = new URLSearchParams(window.location.search).get('application_id');
		if (selected && !reference(selected)) {
			error = 'Invalid application reference.';
			pending = false;
			blocked = true;
		} else {
			application = selected ?? undefined;
			void load();
		}
		return () => {
			mounted = false;
			reveal = null;
		};
	});
	async function load(nextQuery: Query = query, history = cursors, index = position) {
		pending = true;
		error = '';
		page = null;
		mode = null;
		target = null;
		view = null;
		if (scoped && !application) {
			selectApplication = true;
			pending = false;
			return;
		}
		const result = await api.list(kind, {
			...nextQuery,
			after: history[index],
			application_id: application
		});
		if (!mounted) return;
		if (result.kind === 'ready') {
			page = result.data;
			query = nextQuery;
			cursors = history;
			position = index;
			blocked = false;
		} else fail(result.kind);
		pending = false;
	}
	function fail(kind: Failure) {
		error = failureMessage[kind];
		blocked = true;
		if (kind === 'signed-out' || kind === 'forbidden') {
			page = null;
			target = null;
			view = null;
			mode = null;
			reveal = null;
		}
	}
	function searchCatalog(event: SubmitEvent) {
		event.preventDefault();
		if (!pending) void load({ search, status }, [undefined], 0);
	}
	function chooseApplication(item: { id: string; name: string }) {
		application = item.id;
		applicationName = item.name;
		selectApplication = false;
		void load({}, [undefined], 0);
	}
	async function open(item: Item, origin: HTMLElement) {
		if (pending || blocked) return;
		trigger = origin;
		pending = true;
		error = '';
		const result = await api.detail(item);
		if (!mounted) return;
		if (result.kind === 'ready') {
			if ('item' in result.data) {
				view = result.data;
				target = result.data.item;
			} else {
				view = null;
				target = result.data;
			}
			mode = 'detail';
			retiring = false;
		} else fail(result.kind);
		pending = false;
	}
	function create(origin: HTMLElement) {
		if (!page || pending || blocked) return;
		trigger = origin;
		target = null;
		view = null;
		mode = 'create';
	}
	async function close() {
		mode = null;
		target = null;
		view = null;
		retiring = false;
		reveal = null;
		await tick();
		if (trigger?.isConnected) trigger.focus();
	}
	async function registered(result: Write<Registered>) {
		if (result.kind === 'saved') {
			if (result.data.client_secret) {
				mode = null;
				target = null;
				view = null;
				if (document.visibilityState === 'hidden') {
					message =
						'Secret dismissed while the page was hidden. Refresh and rotate after checking the client.';
					blocked = true;
					return;
				}
				reveal = result.data;
				message = 'Store this secret before closing.';
			} else {
				message = 'Change saved.';
				await load();
			}
		} else {
			await close();
			fail(result.kind);
		}
	}
	async function register(command: Command) {
		if (pending || blocked) return;
		pending = true;
		error = '';
		message = '';
		const result = await api.register(command);
		if (!mounted) return;
		pending = false;
		await registered(result);
		await tick();
		notification?.focus();
	}
	async function change(command: Command) {
		const revision = view?.policy_revision ?? page?.policy_revision;
		if (!revision || pending || blocked) return;
		pending = true;
		error = '';
		message = '';
		const result = await api.change(revision, command);
		if (!mounted) return;
		pending = false;
		await close();
		if (result.kind === 'saved') {
			message = 'Access policy saved.';
			await load();
		} else fail(result.kind);
		await tick();
		notification?.focus();
	}
	function save(command: Command) {
		if (kind === 'roles' || kind === 'capabilities') void change(command);
		else void register(command);
	}
	async function acknowledge() {
		await close();
		message = 'Secret dismissed. It cannot be retrieved again.';
		await load();
	}
	function hideSecret() {
		if (reveal) {
			reveal = null;
			message = 'Secret dismissed. Refresh the catalog before making another change.';
			blocked = true;
		}
	}
	function location(kind: Kind, id: string) {
		return `/console/${kind}?application_id=${encodeURIComponent(id)}` as const;
	}
</script>

<svelte:window onpagehide={hideSecret} />
<svelte:document
	onvisibilitychange={() => {
		if (document.visibilityState === 'hidden') hideSecret();
	}}
/>
<div class="directory-heading">
	<div>
		<p class="eyebrow">IDENTITY / APPLICATION DIRECTORY</p>
		<h1>{titles[kind]}</h1>
		<p class="muted">
			{kind === 'applications'
				? 'Connect applications to one trusted identity.'
				: application
					? `Application: ${applicationName || application}`
					: 'Shared definitions with explicit application bindings.'}
		</p>
	</div>
	<Button variant="outline" disabled={pending || !!reveal} onclick={() => load()}
		>Refresh catalog</Button
	>
</div>
{#if message}<p class="admin-success" role="status" tabindex="-1" bind:this={notification}>
		{message}
	</p>{/if}
{#if error}<p class="admin-error" role="alert" tabindex="-1" bind:this={notification}>{error}</p>
	<a class="admin-link" href={resolve('/')}>My account / sign in</a>{/if}
{#if scoped || application}<div class="catalog-context">
		<Button
			variant="outline"
			disabled={pending || !!reveal}
			onclick={() => (selectApplication = !selectApplication)}>Choose application</Button
		>{#if !scoped && application}<Button
				variant="outline"
				disabled={pending || !!reveal}
				onclick={() => {
					application = undefined;
					applicationName = '';
					void load({}, [undefined], 0);
				}}>Show shared catalog</Button
			>{/if}
	</div>{/if}
{#if selectApplication}<CatalogPicker
		label="Applications"
		load={(search, after) => api.list('applications', { search, after })}
		choose={chooseApplication}
		disabled={pending || !!reveal}
	/>{/if}
<form class="directory-toolbar" aria-label="Search catalog" onsubmit={searchCatalog}>
	<div class="search-field">
		<label for="catalog-search">Search {titles[kind].toLowerCase()}</label><input
			id="catalog-search"
			bind:value={search}
			maxlength="100"
			placeholder="Name prefix"
			disabled={pending || !page || !!reveal}
			autocomplete="off"
		/>
	</div>
	{#if kind === 'applications' || kind === 'clients' || kind === 'capabilities'}<div>
			<label for="catalog-status">Status</label><select
				id="catalog-status"
				bind:value={status}
				disabled={pending || !page || !!reveal}
				><option value="">All</option><option value="active">Active</option><option value="inactive"
					>{kind === 'capabilities' ? 'Retired' : 'Inactive'}</option
				></select
			>
		</div>{/if}<Button type="submit" variant="outline" disabled={pending || !page || !!reveal}
		>Search</Button
	><Button
		type="button"
		disabled={pending || blocked || !page || !!reveal}
		onclick={(event) => create(event.currentTarget)}
		>Create {kind === 'capabilities' ? 'capability' : kind.slice(0, -1)}</Button
	>
</form>
{#if pending}<p role="status">Loading catalog…</p>{/if}
{#if page}<!-- svelte-ignore a11y_no_noninteractive_tabindex (keyboard users must be able to scroll the table horizontally) -->
	<div class="directory-table" role="region" aria-label="Catalog table" tabindex="0">
		<table>
			<caption class="sr-only">{titles[kind]}</caption><thead
				><tr
					><th scope="col">Name</th><th scope="col">Reference</th><th scope="col">Status</th><th
						scope="col">Actions</th
					></tr
				></thead
			><tbody
				>{#each page.items as item (item.id)}<tr
						><td
							>{item.name}{#if item.owner_email}<span class="user-email">{item.owner_email}</span
								>{/if}</td
						><td class="catalog-id">{item.id}</td><td
							>{item.active === undefined
								? '—'
								: item.active
									? 'Active'
									: item.kind === 'capability'
										? 'Retired'
										: 'Inactive'}</td
						><td
							><Button
								variant="outline"
								disabled={pending || blocked || !!reveal}
								onclick={(event) => open(item, event.currentTarget)}
								aria-label={`View ${item.name}`}>View</Button
							></td
						></tr
					>{:else}<tr><td colspan="4">No matching records.</td></tr>{/each}</tbody
			>
		</table>
	</div>
	<div class="directory-pagination">
		<Button
			variant="outline"
			disabled={pending || position === 0 || !!reveal}
			onclick={() => load(query, cursors, position - 1)}>Previous</Button
		><span>Page {position + 1}</span><Button
			variant="outline"
			disabled={pending || !page.next || !!reveal}
			onclick={() => load(query, [...cursors.slice(0, position + 1), page!.next!], position + 1)}
			>Next</Button
		>
	</div>{/if}
{#if mode}<Modal
		title={mode === 'create'
			? `Create ${kind === 'capabilities' ? 'capability' : kind.slice(0, -1)}`
			: mode === 'edit'
				? `Edit ${target?.name}`
				: (target?.name ?? 'Details')}
		{pending}
		{close}
	>
		{#if mode === 'create' || mode === 'edit'}<RegistrationEditor
				{kind}
				{application}
				{target}
				{api}
				{pending}
				{save}
				cancel={close}
			/>
		{:else if target}<p class="catalog-id">{target.id}</p>
			{#if target.kind === 'application'}<p>Owner: {target.owner_email}</p>
				<p>Status: {target.active ? 'Active' : 'Inactive'}</p>
				<nav class="catalog-tabs" aria-label="Application catalogs">
					{#each ['clients', 'resources', 'scopes', 'roles', 'capabilities'] as section (section)}<a
							class="admin-link"
							href={resolve(location(section as Kind, target.id))}>{titles[section as Kind]}</a
						>{/each}
				</nav>
				<Button variant="outline" disabled={pending} onclick={() => (mode = 'edit')}
					>Edit application</Button
				>
			{:else if target.kind === 'client'}<h3>Callback URLs</h3>
				<ul>
					{#each target.redirect_uris ?? [] as uri (uri)}<li class="catalog-id">{uri}</li>{/each}
				</ul>
				<p>Refresh tokens: {target.refresh_tokens ? 'Allowed' : 'Disabled'}</p>
				<Button variant="outline" disabled={pending} onclick={() => (mode = 'edit')}
					>Edit client</Button
				><ClientCredentials client={target} {pending} save={register} />
			{:else if view}<CatalogBindings
					{view}
					{api}
					{pending}
					save={change}
				/>{#if target.kind === 'capability' && target.active}<Button
						variant="outline"
						disabled={pending}
						onclick={() => (retiring = true)}>Retire capability</Button
					>{#if retiring}<section class="catalog-confirm">
							<p>
								Retire {target.name} permanently? New authorization checks will no longer grant this capability.
							</p>
							<Button variant="outline" disabled={pending} onclick={() => (retiring = false)}
								>Cancel retirement</Button
							><Button
								disabled={pending}
								onclick={() =>
									change({ operation: 'retire_capability', capability_id: target!.id })}
								>Confirm capability retirement</Button
							>
						</section>{/if}{/if}{/if}
			<div class="modal-actions">
				<Button variant="outline" disabled={pending} onclick={close}>Close</Button>
			</div>{/if}
	</Modal>{/if}
{#if reveal}<Modal title="Save the client secret" close={acknowledge}
		><p>
			This secret is shown once. Store it in your application’s secret manager before closing.
			Switching away from this page clears the reveal.
		</p>
		<p class="catalog-id">Client ID: {reveal.record.id}</p>
		<label for="revealed-secret">Client secret</label><textarea
			id="revealed-secret"
			class="catalog-secret"
			readonly
			value={reveal.client_secret}
			rows="3"
			spellcheck="false"
			autocomplete="off"></textarea>
		<div class="modal-actions">
			<Button onclick={acknowledge}>I have stored the secret</Button>
		</div></Modal
	>{/if}
