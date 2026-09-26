<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	import { onMount, tick } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import type { User, Change } from '$lib/admin/directory';
	let {
		user,
		pending,
		save,
		close
	}: { user: User; pending: boolean; save: (change: Change) => void; close: () => void } = $props();
	let first = $state(''),
		last = $state('');
	let firstInput: HTMLInputElement;
	onMount(() => {
		first = user.first_name;
		last = user.last_name;
		void tick().then(() => {
			if (firstInput?.isConnected) firstInput.focus();
		});
	});
	function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!pending) save({ kind: 'names', first_name: first, last_name: last });
	}
</script>

<form class="admin-form" onsubmit={submit}>
	<label for="first-name">{$language.t('profile.firstName')}</label><input
		id="first-name"
		bind:this={firstInput}
		bind:value={first}
		required
		maxlength="100"
		disabled={pending}
		autocomplete="off"
	/>
	<label for="last-name">{$language.t('profile.lastName')}</label><input
		id="last-name"
		bind:value={last}
		required
		maxlength="100"
		disabled={pending}
		autocomplete="off"
	/>
	<p class="muted">{$language.t('directory.nameHelp')}</p>
	<div class="modal-actions">
		<Button variant="outline" onclick={close} disabled={pending}
			>{$language.t('common.cancel')}</Button
		><Button type="submit" disabled={pending}
			>{$language.t(pending ? 'common.saving' : 'directory.saveName')}</Button
		>
	</div>
</form>
