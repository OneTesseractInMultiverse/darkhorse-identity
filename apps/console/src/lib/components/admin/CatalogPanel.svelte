<script lang="ts">
	import { useCatalogLocalization } from '$lib/i18n/catalog-context';
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	const catalog = useCatalogLocalization();
	import { onMount, tick } from 'svelte';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import {
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
	import ApplicationDetails from './ApplicationDetails.svelte';
	import ClientDetails from './ClientDetails.svelte';
	let { kind, api }: { kind: Kind; api: CatalogApi } = $props();
	let page = $state<Page | null>(null),
		pending = $state(true),
		blocked = $state(false),
		error = $state<Failure | 'reference' | null>(null),
		message = $state<'hidden' | 'store' | 'saved' | 'policy' | 'dismissed' | 'refresh' | null>(
			null
		),
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
			error = 'reference';
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
		error = null;
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
		error = kind;
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
		error = null;
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
					message = 'hidden';
					blocked = true;
					return;
				}
				reveal = result.data;
				message = 'store';
			} else {
				message = 'saved';
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
		error = null;
		message = null;
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
		error = null;
		message = null;
		const result = await api.change(revision, command);
		if (!mounted) return;
		pending = false;
		await close();
		if (result.kind === 'saved') {
			message = 'policy';
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
		message = 'dismissed';
		await load();
	}
	function hideSecret() {
		if (reveal) {
			reveal = null;
			message = 'refresh';
			blocked = true;
		}
	}
</script>

<svelte:window onpagehide={hideSecret} />
<svelte:document
	onvisibilitychange={() => {
		if (document.visibilityState === 'hidden') hideSecret();
	}}
/>
<div lang={$catalog.locale}>
	<div class="directory-heading">
		<div>
			<p class="eyebrow">{$catalog.t('eyebrow')}</p>
			<h1>{$language.t(`console.${kind}`)}</h1>
			<p class="muted catalog-intro">{$catalog.t(`intro.${kind}`)}</p>
			{#if application}<p class="catalog-help catalog-id">
					{$catalog.t('applicationValue', { value: applicationName || application })}
				</p>{/if}
		</div>
		<Button variant="outline" disabled={pending || !!reveal} onclick={() => load()}
			>{$catalog.t('refresh')}</Button
		>
	</div>
	{#if message}<p class="admin-success" role="status" tabindex="-1" bind:this={notification}>
			{$catalog.t(`message.${message}`)}
		</p>{/if}
	{#if error}<p class="admin-error" role="alert" tabindex="-1" bind:this={notification}>
			{$catalog.t(`error.${error}`)}
		</p>
		<a class="admin-link" href={resolve('/')}>{$catalog.t('account')}</a>{/if}
	{#if scoped || application}<div class="catalog-context">
			<Button
				variant="outline"
				disabled={pending || !!reveal}
				onclick={() => (selectApplication = !selectApplication)}
				>{$catalog.t('chooseApplication')}</Button
			>{#if !scoped && application}<Button
					variant="outline"
					disabled={pending || !!reveal}
					onclick={() => {
						application = undefined;
						applicationName = '';
						void load({}, [undefined], 0);
					}}>{$catalog.t('sharedCatalog')}</Button
				>{/if}
		</div>{/if}
	{#if selectApplication}<CatalogPicker
			label={$language.t('console.applications')}
			load={(search, after) => api.list('applications', { search, after })}
			choose={chooseApplication}
			disabled={pending || !!reveal}
		/>{/if}
	<form class="directory-toolbar" aria-label={$catalog.t('searchForm')} onsubmit={searchCatalog}>
		<div class="search-field">
			<label for="catalog-search">{$catalog.t(`search.${kind}`)}</label><input
				id="catalog-search"
				bind:value={search}
				maxlength="100"
				placeholder={$catalog.t('searchPlaceholder')}
				disabled={pending || !page || !!reveal}
				autocomplete="off"
			/>
		</div>
		{#if kind === 'applications' || kind === 'clients' || kind === 'capabilities'}<div>
				<label for="catalog-status">{$language.t('common.status')}</label><select
					id="catalog-status"
					bind:value={status}
					disabled={pending || !page || !!reveal}
					><option value="">{$catalog.t('all')}</option><option value="active"
						>{$language.t('common.active')}</option
					><option value="inactive"
						>{kind === 'capabilities'
							? $catalog.t('retired')
							: $language.t('common.inactive')}</option
					></select
				>
			</div>{/if}<Button type="submit" variant="outline" disabled={pending || !page || !!reveal}
			>{$language.t('directory.search')}</Button
		><Button
			type="button"
			disabled={pending || blocked || !page || !!reveal}
			onclick={(event) => create(event.currentTarget)}>{$catalog.t(`create.${kind}`)}</Button
		>
	</form>
	{#if pending}<p role="status">{$catalog.t('loading')}</p>{/if}
	{#if page}<!-- svelte-ignore a11y_no_noninteractive_tabindex (keyboard users must be able to scroll the table horizontally) -->
		<div class="directory-table" role="region" aria-label={$catalog.t('table')} tabindex="0">
			<table>
				<caption class="sr-only">{$language.t(`console.${kind}`)}</caption><thead
					><tr
						><th scope="col">{$catalog.t('name')}</th><th scope="col">{$catalog.t('reference')}</th
						><th scope="col">{$language.t('common.status')}</th><th scope="col"
							>{$language.t('common.actions')}</th
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
										? $language.t('common.active')
										: item.kind === 'capability'
											? $catalog.t('retired')
											: $language.t('common.inactive')}</td
							><td
								><Button
									variant="outline"
									disabled={pending || blocked || !!reveal}
									onclick={(event) => open(item, event.currentTarget)}
									aria-label={$catalog.t('viewName', { name: item.name })}
									>{$language.t('directory.view')}</Button
								></td
							></tr
						>{:else}<tr><td colspan="4">{$catalog.t('empty')}</td></tr>{/each}</tbody
				>
			</table>
		</div>
		<div class="directory-pagination">
			<Button
				variant="outline"
				disabled={pending || position === 0 || !!reveal}
				onclick={() => load(query, cursors, position - 1)}>{$catalog.t('previous')}</Button
			><span>{$catalog.t('page', { number: position + 1 })}</span><Button
				variant="outline"
				disabled={pending || !page.next || !!reveal}
				onclick={() => load(query, [...cursors.slice(0, position + 1), page!.next!], position + 1)}
				>{$catalog.t('next')}</Button
			>
		</div>{/if}
	{#if mode}<Modal
			wide
			description={mode === 'detail'
				? kind === 'applications'
					? $catalog.t('detailDescription')
					: $catalog.t(`intro.${kind}`)
				: $catalog.t('requiredHelp')}
			title={mode === 'create'
				? $catalog.t(`create.${kind}`)
				: mode === 'edit'
					? $catalog.t('editName', { name: target?.name ?? $catalog.t('details') })
					: (target?.name ?? $catalog.t('details'))}
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
			{:else if target}
				{#if target.kind === 'application'}<ApplicationDetails application={target} />
					<Button variant="outline" disabled={pending} onclick={() => (mode = 'edit')}
						>{$catalog.t('editApplication')}</Button
					>
				{:else if target.kind === 'client'}<ClientDetails client={target} />
					<Button variant="outline" disabled={pending} onclick={() => (mode = 'edit')}
						>{$catalog.t('editClient')}</Button
					><ClientCredentials client={target} {pending} save={register} />
				{:else if view}<p class="catalog-id">{$catalog.t('recordId', { id: target.id })}</p>
					<CatalogBindings
						{view}
						{api}
						{pending}
						save={change}
					/>{#if target.kind === 'capability' && target.active}<Button
							variant="outline"
							disabled={pending}
							onclick={() => (retiring = true)}>{$catalog.t('retireCapability')}</Button
						>{#if retiring}<section class="catalog-confirm">
								<p>
									{$catalog.t('retireHelp', { name: target.name })}
								</p>
								<Button variant="outline" disabled={pending} onclick={() => (retiring = false)}
									>{$catalog.t('cancelRetirement')}</Button
								><Button
									disabled={pending}
									onclick={() =>
										change({ operation: 'retire_capability', capability_id: target!.id })}
									>{$catalog.t('confirmRetirement')}</Button
								>
							</section>{/if}{/if}{/if}
				<div class="modal-actions">
					<Button variant="outline" disabled={pending} onclick={close}
						>{$language.t('common.close')}</Button
					>
				</div>{/if}
		</Modal>{/if}
	{#if reveal}<Modal title={$catalog.t('revealTitle')} close={acknowledge}
			><p>
				{$catalog.t('revealHelp')}
			</p>
			<p class="catalog-help">
				{$catalog.t('revealMethod')}
			</p>
			<p class="catalog-id">{$catalog.t('clientIdValue', { id: reveal.record.id })}</p>
			<label for="revealed-secret">{$catalog.t('secret')}</label><textarea
				id="revealed-secret"
				class="catalog-secret"
				readonly
				value={reveal.client_secret}
				rows="3"
				spellcheck="false"
				autocomplete="off"></textarea>
			<div class="modal-actions">
				<Button onclick={acknowledge}>{$catalog.t('stored')}</Button>
			</div></Modal
		>{/if}
</div>
