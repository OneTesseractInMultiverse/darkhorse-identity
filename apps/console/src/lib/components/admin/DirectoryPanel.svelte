<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import { onMount, tick } from 'svelte';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import {
		type DirectoryApi,
		type Page,
		type User,
		type Query,
		type Change,
		type Failure,
		type Write
	} from '$lib/admin/directory';
	import Modal from './Modal.svelte';
	import ProfileView from './ProfileView.svelte';
	import NameEditor from './NameEditor.svelte';
	import AccessEditor from './AccessEditor.svelte';
	let { api }: { api: DirectoryApi } = $props();
	type Mode = 'profile' | 'names' | 'status' | 'access';
	let page = $state<Page | null>(null),
		pending = $state(true),
		blocked = $state(false),
		error = $state<Failure | Exclude<Write['kind'], 'saved'> | null>(null),
		message = $state(false);
	let search = $state(''),
		status = $state<Query['status']>(''),
		applied = $state<Query>({ search: '', status: '' });
	let cursors = $state<(string | undefined)[]>([undefined]),
		position = $state(0);
	let selected = $state<User | null>(null),
		mode = $state<Mode | null>(null),
		failed = $state<Failure | null>(null);
	let trigger: HTMLElement | null = null,
		mounted = false;
	let notification: HTMLParagraphElement | undefined = $state();
	onMount(() => {
		mounted = true;
		void load();
		return () => {
			mounted = false;
		};
	});
	async function load(query: Query = applied, history = cursors, index = position) {
		pending = true;
		error = null;
		page = null;
		const result = await api.list({ ...query, after: history[index] });
		if (!mounted) return;
		if (result.kind === 'ready') {
			page = result.data;
			applied = query;
			cursors = history;
			position = index;
			blocked = false;
			failed = null;
		} else deny(result.kind);
		pending = false;
	}
	function deny(kind: Failure) {
		failed = kind;
		page = null;
		selected = null;
		mode = null;
		error = kind;
	}
	function searchUsers(event: SubmitEvent) {
		event.preventDefault();
		if (!pending) {
			message = false;
			void load({ search: search.trim(), status }, [undefined], 0);
		}
	}
	function next() {
		if (page?.next && !pending)
			void load(applied, [...cursors.slice(0, position + 1), page.next], position + 1);
	}
	async function open(kind: Mode, target: User, origin: HTMLElement) {
		if (pending || blocked) return;
		trigger = origin;
		pending = true;
		error = null;
		message = false;
		const result = await api.user(target.id);
		if (!mounted) return;
		if (result.kind === 'ready') {
			selected = result.data;
			mode = kind;
		} else deny(result.kind);
		pending = false;
	}
	async function close() {
		mode = null;
		selected = null;
		await tick();
		if (trigger?.isConnected) trigger.focus();
	}
	async function save(change: Change, target = selected) {
		if (!target || pending || blocked) return;
		pending = true;
		error = null;
		message = false;
		const result = await api.update(target, change);
		if (!mounted) return;
		mode = null;
		selected = null;
		pending = false;
		if (result.kind === 'saved') {
			message = true;
			await load();
		} else if (result.kind === 'signed-out' || result.kind === 'forbidden') deny(result.kind);
		else {
			error = result.kind;
			blocked = true;
		}
		await tick();
		notification?.focus();
	}
</script>

<div class="directory-heading">
	<div>
		<p class="eyebrow">{$language.t('directory.eyebrow')}</p>
		<h1>{$language.t('console.users')}</h1>
		<p class="muted">{$language.t('directory.intro')}</p>
	</div>
	<Button variant="outline" onclick={() => load()} disabled={pending}
		>{$language.t('directory.refresh')}</Button
	>
</div>
{#if message}<p class="admin-success" role="status" tabindex="-1" bind:this={notification}>
		{$language.t('directory.saved')}
	</p>{/if}
{#if error}<p class="admin-error" role="alert" tabindex="-1" bind:this={notification}>
		{$language.t(`directory.error.${error}`)}
	</p>{/if}
{#if failed === 'signed-out' || failed === 'forbidden'}<a class="admin-link" href={resolve('/')}
		>{$language.t('directory.signIn')}</a
	>{/if}
<form
	class="directory-toolbar"
	aria-label={$language.t('directory.searchForm')}
	onsubmit={searchUsers}
>
	<div class="search-field">
		<label for="user-search">{$language.t('directory.searchUsers')}</label><input
			id="user-search"
			bind:value={search}
			placeholder={$language.t('directory.searchPlaceholder')}
			maxlength="100"
			disabled={pending}
			autocomplete="off"
		/>
	</div>
	<div>
		<label for="user-status">{$language.t('common.status')}</label><select
			id="user-status"
			bind:value={status}
			disabled={pending}
			><option value="">{$language.t('directory.allStatuses')}</option><option value="active"
				>{$language.t('common.active')}</option
			><option value="inactive">{$language.t('common.inactive')}</option></select
		>
	</div>
	<Button type="submit" disabled={pending}>{$language.t('directory.search')}</Button>
</form>
{#if pending && !selected}<p role="status" class="muted">{$language.t('directory.loading')}</p>{/if}
{#if page}
	<!-- svelte-ignore a11y_no_noninteractive_tabindex (keyboard users must be able to scroll the table horizontally) -->
	<div
		class="directory-table"
		role="region"
		aria-label={$language.t('directory.table')}
		tabindex="0"
	>
		<table>
			<caption class="sr-only">{$language.t('directory.caption')}</caption><thead
				><tr
					><th scope="col">{$language.t('directory.user')}</th><th scope="col"
						>{$language.t('common.status')}</th
					><th scope="col">{$language.t('directory.administrator')}</th><th scope="col"
						>{$language.t('common.actions')}</th
					></tr
				></thead
			><tbody>
				{#each page.items as user (user.id)}
					<tr
						><td
							><strong>{user.first_name} {user.last_name}</strong><span class="user-email"
								>{user.email}</span
							></td
						><td
							><span class:inactive={!user.active} class="status-badge"
								>{$language.t(user.active ? 'common.active' : 'common.inactive')}</span
							></td
						><td
							>{user.administrator ? $language.t('directory.yes') : '—'}{#if user.id === page.actor}
								· {$language.t('directory.you')}{/if}</td
						><td
							><div class="row-actions">
								<Button
									variant="ghost"
									disabled={pending || blocked}
									aria-label={$language.t('directory.viewName', {
										name: `${user.first_name} ${user.last_name}`
									})}
									onclick={(event) => open('profile', user, event.currentTarget)}
									>{$language.t('directory.view')}</Button
								>
								<Button
									variant="outline"
									disabled={pending || blocked}
									aria-label={$language.t('directory.assignName', {
										name: `${user.first_name} ${user.last_name}`
									})}
									onclick={(event) => open('access', user, event.currentTarget)}
									>{$language.t('directory.access')}</Button
								>
								<Button
									variant={user.active ? 'destructive' : 'secondary'}
									disabled={pending || blocked}
									aria-label={$language.t(
										user.active ? 'directory.deactivateName' : 'directory.reactivateName',
										{ name: `${user.first_name} ${user.last_name}` }
									)}
									onclick={(event) => open('status', user, event.currentTarget)}
									>{$language.t(
										user.active ? 'directory.deactivate' : 'directory.reactivate'
									)}</Button
								>
							</div></td
						></tr
					>
				{/each}
			</tbody>
		</table>
		{#if page.items.length === 0}<p class="directory-empty">
				{$language.t('directory.empty')}
			</p>{/if}
	</div>
	<div class="directory-pagination">
		<span>{$language.t('directory.count', { count: page.items.length })}</span>
		<div>
			<Button
				variant="ghost"
				onclick={() => load(applied, cursors, position - 1)}
				disabled={pending || position === 0}>{$language.t('directory.previous')}</Button
			><Button variant="outline" onclick={next} disabled={pending || !page.next}
				>{$language.t('directory.next')}</Button
			>
		</div>
	</div>
{/if}
{#if mode && selected}
	<Modal
		title={$language.t(
			mode === 'profile'
				? 'directory.profile'
				: mode === 'names'
					? 'directory.editName'
					: mode === 'access'
						? 'directory.applicationAccess'
						: selected.active
							? 'directory.deactivateTitle'
							: 'directory.reactivateTitle'
		)}
		{pending}
		{close}
	>
		{#if mode === 'profile'}<ProfileView
				user={selected}
				edit={() => {
					mode = 'names';
				}}
				{close}
			/>
		{:else if mode === 'names'}<NameEditor user={selected} {pending} {save} {close} />
		{:else if mode === 'access'}<AccessEditor
				user={selected}
				{api}
				{pending}
				{save}
				{close}
				denied={deny}
			/>
		{:else}
			<p>
				{selected.first_name}
				{selected.last_name} <span class="muted">({selected.email})</span>
			</p>
			<p class="muted">
				{$language.t(selected.active ? 'directory.deactivateHelp' : 'directory.reactivateHelp')}
			</p>
			<div class="modal-actions">
				<Button variant="outline" onclick={close} disabled={pending}
					>{$language.t('common.cancel')}</Button
				><Button
					variant={selected.active ? 'destructive' : 'default'}
					onclick={() => save({ kind: 'status', active: !selected!.active })}
					disabled={pending}
					>{$language.t(
						pending
							? 'common.saving'
							: selected.active
								? 'directory.confirmDeactivation'
								: 'directory.confirmReactivation'
					)}</Button
				>
			</div>
		{/if}
	</Modal>
{/if}
