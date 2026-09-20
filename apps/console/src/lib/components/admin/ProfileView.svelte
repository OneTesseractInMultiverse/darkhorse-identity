<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import type { User } from '$lib/admin/directory';
	let { user, edit, close }: { user: User; edit: () => void; close: () => void } = $props();
</script>

<dl class="profile-fields">
	<div>
		<dt>Name</dt>
		<dd>{user.first_name} {user.last_name}</dd>
	</div>
	<div>
		<dt>Email / username</dt>
		<dd>{user.email}</dd>
	</div>
	<div>
		<dt>Status</dt>
		<dd>{user.active ? 'Active' : 'Inactive'}</dd>
	</div>
	<div>
		<dt>Email ownership</dt>
		<dd>{user.email_verified ? 'Verified' : 'Unverified'}</dd>
	</div>
	<div>
		<dt>Platform administrator</dt>
		<dd>{user.administrator ? 'Yes' : 'No'}</dd>
	</div>
</dl>
<div class="modal-actions">
	<Button variant="outline" onclick={close}>Close</Button><Button onclick={edit}>Edit name</Button>
</div>
