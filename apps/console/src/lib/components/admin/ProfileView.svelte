<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import type { User } from '$lib/admin/directory';
	let { user, edit, close }: { user: User; edit: () => void; close: () => void } = $props();
</script>

<dl class="profile-fields">
	<div>
		<dt>{$language.t('directory.name')}</dt>
		<dd>{user.first_name} {user.last_name}</dd>
	</div>
	<div>
		<dt>{$language.t('directory.email')}</dt>
		<dd>{user.email}</dd>
	</div>
	<div>
		<dt>{$language.t('common.status')}</dt>
		<dd>{$language.t(user.active ? 'common.active' : 'common.inactive')}</dd>
	</div>
	<div>
		<dt>{$language.t('directory.emailOwnership')}</dt>
		<dd>{$language.t(user.email_verified ? 'profile.verified' : 'profile.unverified')}</dd>
	</div>
	<div>
		<dt>{$language.t('directory.platformAdministrator')}</dt>
		<dd>{$language.t(user.administrator ? 'directory.yes' : 'directory.no')}</dd>
	</div>
</dl>
<p>
	<a href={resolve(`/console/profile?user=${user.id}`)}>{$language.t('directory.fullProfile')}</a>
</p>
<div class="modal-actions">
	<Button variant="outline" onclick={close}>{$language.t('common.close')}</Button><Button
		onclick={edit}>{$language.t('directory.editName')}</Button
	>
</div>
