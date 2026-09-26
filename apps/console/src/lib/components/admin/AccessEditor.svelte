<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import {
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
		error = $state<Failure | null>(null),
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
		error = null;
		const result = await api.access(user.id, application);
		if (!mounted) return;
		if (result.kind === 'ready') {
			view = result.data;
			roleId = view.roles[0]?.id ?? '';
			assigned = view.roles[0]?.assigned ?? false;
		} else {
			view = null;
			error = result.kind;
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

{#if loading}<p role="status">{$language.t('directory.loadingRoles')}</p>{/if}
{#if error}<p role="alert" class="admin-error">{$language.t(`directory.error.${error}`)}</p>
	<Button onclick={() => load()}>{$language.t('directory.retryAccess')}</Button>{/if}
{#if view}
	<form class="admin-form" onsubmit={submit}>
		{#if view.applications.length}
			<label for="role-application">{$language.t('directory.application')}</label><select
				id="role-application"
				value={view.selected ?? ''}
				onchange={(event) => load(event.currentTarget.value)}
				disabled={loading || pending}
				>{#each view.applications as app (app.id)}<option value={app.id}
						>{app.active
							? app.name
							: $language.t('directory.inactiveApplication', { name: app.name })}</option
					>{/each}</select
			>
			{#if view.roles.length}
				<label for="application-role">{$language.t('directory.role')}</label><select
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
					/>
					{$language.t('directory.assignRole')}</label
				>
				{#if !activeApplication?.active}<p class="muted">
						{$language.t('directory.inactiveHelp')}
					</p>{/if}
			{:else}<p class="muted">{$language.t('directory.noRoles')}</p>{/if}
		{:else}<p class="muted">{$language.t('directory.noApplications')}</p>{/if}
		<div class="modal-actions">
			<Button variant="outline" onclick={close} disabled={pending}
				>{$language.t('common.cancel')}</Button
			><Button
				type="submit"
				disabled={pending || loading || !roleId || (!activeApplication?.active && assigned)}
				>{$language.t(pending ? 'common.saving' : 'directory.saveAccess')}</Button
			>
		</div>
	</form>
{:else if !loading}<div class="modal-actions">
		<Button variant="outline" onclick={close}>{$language.t('common.close')}</Button>
	</div>{/if}
