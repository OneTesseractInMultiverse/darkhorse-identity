<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import {
		failureMessage,
		type DirectoryApi,
		type Page,
		type User,
		type Query,
		type Change,
		type Failure
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
		error = $state(''),
		message = $state('');
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
		error = '';
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
		error = failureMessage[kind];
	}
	function searchUsers(event: SubmitEvent) {
		event.preventDefault();
		if (!pending) {
			message = '';
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
		error = '';
		message = '';
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
		error = '';
		message = '';
		const result = await api.update(target, change);
		if (!mounted) return;
		mode = null;
		selected = null;
		pending = false;
		if (result.kind === 'saved') {
			message = 'Change saved.';
			await load();
		} else if (result.kind === 'signed-out' || result.kind === 'forbidden') deny(result.kind);
		else {
			error = failureMessage[result.kind];
			blocked = true;
		}
		await tick();
		notification?.focus();
	}
</script>

<div class="directory-heading">
	<div>
		<p class="eyebrow">IDENTITY / DIRECTORY</p>
		<h1>User directory</h1>
		<p class="muted">The people behind every connection.</p>
	</div>
	<Button variant="outline" onclick={() => load()} disabled={pending}>Refresh directory</Button>
</div>
{#if message}<p class="admin-success" role="status" tabindex="-1" bind:this={notification}>
		{message}
	</p>{/if}
{#if error}<p class="admin-error" role="alert" tabindex="-1" bind:this={notification}>
		{error}
	</p>{/if}
{#if failed === 'signed-out' || failed === 'forbidden'}<a class="admin-link" href={resolve('/')}
		>Return to sign in</a
	>{/if}
<form class="directory-toolbar" aria-label="Search directory" onsubmit={searchUsers}>
	<div class="search-field">
		<label for="user-search">Search users</label><input
			id="user-search"
			bind:value={search}
			placeholder="Name or email prefix"
			maxlength="100"
			disabled={pending}
			autocomplete="off"
		/>
	</div>
	<div>
		<label for="user-status">Status</label><select
			id="user-status"
			bind:value={status}
			disabled={pending}
			><option value="">All statuses</option><option value="active">Active</option><option
				value="inactive">Inactive</option
			></select
		>
	</div>
	<Button type="submit" disabled={pending}>Search</Button>
</form>
{#if pending && !selected}<p role="status" class="muted">Loading directory…</p>{/if}
{#if page}
	<!-- svelte-ignore a11y_no_noninteractive_tabindex (keyboard users must be able to scroll the table horizontally) -->
	<div class="directory-table" role="region" aria-label="User directory table" tabindex="0">
		<table>
			<caption class="sr-only">Organization users, oldest accounts first</caption><thead
				><tr
					><th scope="col">User</th><th scope="col">Status</th><th scope="col">Administrator</th><th
						scope="col">Actions</th
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
								>{user.active ? 'Active' : 'Inactive'}</span
							></td
						><td>{user.administrator ? 'Yes' : '—'}{user.id === page.actor ? ' · You' : ''}</td><td
							><div class="row-actions">
								<Button
									variant="ghost"
									disabled={pending || blocked}
									aria-label={`View ${user.first_name} ${user.last_name}`}
									onclick={(event) => open('profile', user, event.currentTarget)}>View</Button
								>
								<Button
									variant="outline"
									disabled={pending || blocked}
									aria-label={`Assign access to ${user.first_name} ${user.last_name}`}
									onclick={(event) => open('access', user, event.currentTarget)}>Access</Button
								>
								<Button
									variant={user.active ? 'destructive' : 'secondary'}
									disabled={pending || blocked}
									aria-label={`${user.active ? 'Deactivate' : 'Reactivate'} ${user.first_name} ${user.last_name}`}
									onclick={(event) => open('status', user, event.currentTarget)}
									>{user.active ? 'Deactivate' : 'Reactivate'}</Button
								>
							</div></td
						></tr
					>
				{/each}
			</tbody>
		</table>
		{#if page.items.length === 0}<p class="directory-empty">
				No users match this page. Change the filters or return to an earlier page.
			</p>{/if}
	</div>
	<div class="directory-pagination">
		<span>{page.items.length} users shown · Oldest first</span>
		<div>
			<Button
				variant="ghost"
				onclick={() => load(applied, cursors, position - 1)}
				disabled={pending || position === 0}>Previous page</Button
			><Button variant="outline" onclick={next} disabled={pending || !page.next}>Next page</Button>
		</div>
	</div>
{/if}
{#if mode && selected}
	<Modal
		title={mode === 'profile'
			? 'User profile'
			: mode === 'names'
				? 'Edit name'
				: mode === 'access'
					? 'Application access'
					: selected.active
						? 'Deactivate account?'
						: 'Reactivate account?'}
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
				{selected.active
					? 'This account will lose sign-in and access immediately. Its profile and assignments will be retained.'
					: 'This account can sign in again. Previously revoked sessions remain invalid.'}
			</p>
			<div class="modal-actions">
				<Button variant="outline" onclick={close} disabled={pending}>Cancel</Button><Button
					variant={selected.active ? 'destructive' : 'default'}
					onclick={() => save({ kind: 'status', active: !selected!.active })}
					disabled={pending}
					>{pending
						? 'Saving…'
						: selected.active
							? 'Confirm deactivation'
							: 'Confirm reactivation'}</Button
				>
			</div>
		{/if}
	</Modal>
{/if}
