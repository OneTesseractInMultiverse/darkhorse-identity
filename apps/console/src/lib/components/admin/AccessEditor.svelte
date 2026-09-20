<script lang="ts">
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import {
		failureMessage,
		type DirectoryApi,
		type User,
		type Access,
		type Change,
		type Failure
	} from '$lib/admin/directory';
	let {
		user,
		api,
		pending,
		save,
		close,
		denied
	}: {
		user: User;
		api: DirectoryApi;
		pending: boolean;
		save: (change: Change, user: User) => void;
		close: () => void;
		denied: (kind: Failure) => void;
	} = $props();
	let view = $state<Access | null>(null),
		loading = $state(true),
		error = $state(''),
		roleId = $state(''),
		assigned = $state(false);
	let mounted = false;
	const activeApplication = $derived(view?.applications.find((app) => app.id === view?.selected));
	onMount(() => {
		mounted = true;
		void load();
		return () => {
			mounted = false;
		};
	});
	async function load(application?: string) {
		loading = true;
		error = '';
		const result = await api.access(user.id, application);
		if (!mounted) return;
		if (result.kind === 'ready') {
			view = result.data;
			roleId = view.roles[0]?.id ?? '';
			assigned = view.roles[0]?.assigned ?? false;
		} else {
			view = null;
			error = failureMessage[result.kind];
			if (result.kind !== 'unavailable') denied(result.kind);
		}
		loading = false;
	}
	function chooseRole(event: Event) {
		roleId = (event.target as HTMLSelectElement).value;
		assigned = view?.roles.find((role) => role.id === roleId)?.assigned ?? false;
	}
	function submit(event: SubmitEvent) {
		event.preventDefault();
		if (pending || loading || !view?.selected || !roleId) return;
		save(
			{
				kind: 'role',
				application_id: view.selected,
				role_id: roleId,
				assigned,
				policy_revision: view.policy_revision
			},
			view.user
		);
	}
</script>

{#if loading}<p role="status">Loading application roles…</p>{/if}
{#if error}<p role="alert" class="admin-error">{error}</p>
	<Button onclick={() => load()}>Retry access lookup</Button>{/if}
{#if view}
	<form class="admin-form" onsubmit={submit}>
		{#if view.applications.length}
			<label for="role-application">Application</label><select
				id="role-application"
				value={view.selected ?? ''}
				onchange={(event) => load(event.currentTarget.value)}
				disabled={loading || pending}
				>{#each view.applications as app (app.id)}<option value={app.id}
						>{app.name}{app.active ? '' : ' (inactive)'}</option
					>{/each}</select
			>
			{#if view.roles.length}
				<label for="application-role">Role</label><select
					id="application-role"
					value={roleId}
					onchange={chooseRole}
					disabled={loading || pending}
					>{#each view.roles as role (role.id)}<option value={role.id}>{role.name}</option
						>{/each}</select
				>
				<label class="checkbox-field"
					><input
						type="checkbox"
						bind:checked={assigned}
						disabled={loading ||
							pending ||
							(!activeApplication?.active &&
								!view.roles.find((role) => role.id === roleId)?.assigned)}
					/> Assign this role</label
				>
				{#if !activeApplication?.active}<p class="muted">
						This application is inactive. You can remove an existing assignment.
					</p>{/if}
			{:else}<p class="muted">No roles are available for this application.</p>{/if}
		{:else}<p class="muted">No applications have been registered.</p>{/if}
		<div class="modal-actions">
			<Button variant="outline" onclick={close} disabled={pending}>Cancel</Button><Button
				type="submit"
				disabled={pending || loading || !roleId || (!activeApplication?.active && assigned)}
				>{pending ? 'Saving…' : 'Save access'}</Button
			>
		</div>
	</form>
{:else if !loading}<div class="modal-actions">
		<Button variant="outline" onclick={close}>Close</Button>
	</div>{/if}
