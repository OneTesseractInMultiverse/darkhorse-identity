<script lang="ts">
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import {
		failureMessage,
		type KeyApi,
		type Options,
		type Eligible,
		type Selection,
		type Creation
	} from '$lib/personal-keys';
	let {
		read,
		create,
		cancel,
		pending
	}: {
		read: KeyApi['options'];
		create: (input: Creation) => Promise<void>;
		cancel: () => void;
		pending: boolean;
	} = $props();
	let options = $state<Options | null>(null);
	let choices = $state<{ resource: Eligible; selection: Selection }[]>([]);
	let name = $state('');
	let expiration = $state('default');
	let days = $state(30);
	let loading = $state(true);
	let message = $state('');
	let blocked = $state(false);
	const application = $derived(choices[0]?.resource.application_id);
	const valid = $derived(
		name.trim().length > 0 &&
			choices.length > 0 &&
			choices.every((c) => c.selection.kind === 'all' || c.selection.capabilities.length > 0)
	);
	onMount(() => {
		void load();
	});
	async function load(after?: string) {
		loading = true;
		message = '';
		const result = await read(after);
		if (result.kind === 'ready') {
			if (after && options && options.policy_revision !== result.value.policy_revision) {
				choices = [];
				blocked = true;
				message = 'Permissions changed. Refresh permissions and select resources again.';
			} else if (!after) {
				choices = [];
				blocked = false;
			}
			options = result.value;
		} else {
			options = null;
			choices = [];
			blocked = true;
			message = failureMessage(result);
		}
		loading = false;
	}
	function select(resource: Eligible, selected: boolean) {
		choices = selected
			? [...choices, { resource, selection: { kind: 'all' } }]
			: choices.filter((c) => c.resource.resource_id !== resource.resource_id);
	}
	function mode(resource: string, all: boolean) {
		choices = choices.map((c) =>
			c.resource.resource_id === resource
				? { ...c, selection: all ? { kind: 'all' } : { kind: 'subset', capabilities: [] } }
				: c
		);
	}
	function capability(resource: string, id: string, checked: boolean) {
		choices = choices.map((c) =>
			c.resource.resource_id !== resource || c.selection.kind === 'all'
				? c
				: {
						...c,
						selection: {
							kind: 'subset',
							capabilities: checked
								? [...c.selection.capabilities, id]
								: c.selection.capabilities.filter((v) => v !== id)
						}
					}
		);
	}
	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!options || !application || !valid || pending || loading || blocked) return;
		await create({
			name: name.trim(),
			application_id: application,
			policy_revision: options.policy_revision,
			expiration:
				expiration === 'days'
					? { kind: 'days', days }
					: { kind: expiration === 'never' ? 'never' : 'default' },
			grants: choices.map((c) => ({ resource_id: c.resource.resource_id, selection: c.selection }))
		});
	}
</script>

<form class="key-editor" aria-label="Create personal API key" onsubmit={submit}>
	<p>
		Choose resources from one application. Permissions are fixed at creation and remain limited by
		your current access.
	</p>
	<label for="key-name">Key name</label><input
		id="key-name"
		bind:value={name}
		required
		maxlength="100"
		autocomplete="off"
		disabled={pending}
	/>
	<label for="key-expiration">Expiration</label><select
		id="key-expiration"
		bind:value={expiration}
		disabled={pending || loading}
	>
		<option value="default">Organization default ({options?.policy.default_days ?? 30} days)</option
		><option value="days">Choose a duration</option>{#if options?.policy.allow_never}<option
				value="never">No expiration</option
			>{/if}
	</select>
	{#if expiration === 'days'}<label for="key-days">Days until expiration</label><input
			id="key-days"
			type="number"
			required
			min="1"
			max={options?.policy.maximum_days ?? 365}
			bind:value={days}
			disabled={pending}
		/>{/if}
	{#if expiration === 'never'}<p class="key-hint">
			This key remains valid until revoked or invalidated by an account security change. Prefer an
			expiration when possible.
		</p>{/if}
	<div class="directory-actions">
		<h3>Resources and permissions</h3>
		<Button type="button" variant="outline" disabled={pending || loading} onclick={() => load()}
			>Refresh permissions</Button
		>
	</div>
	{#if message}<p role="alert">{message}</p>{/if}
	{#if loading}<p role="status">Loading your permissions…</p>{:else if options}
		{#if options.items.length === 0}<p>
				You have no delegable resource access. An administrator must assign an application role
				first.
			</p>{/if}
		{#each options.items as resource (resource.resource_id)}
			{@const selected = choices.find((c) => c.resource.resource_id === resource.resource_id)}
			<fieldset class="key-resource" disabled={pending || blocked}>
				<legend>{resource.application_name} / {resource.resource_name}</legend>
				<label class="key-check"
					><input
						type="checkbox"
						aria-label={`Include ${resource.resource_name}`}
						checked={!!selected}
						disabled={!selected &&
							((application !== undefined && application !== resource.application_id) ||
								choices.length >= 16)}
						onchange={(e) => select(resource, e.currentTarget.checked)}
					/>Include this resource</label
				>
				{#if selected}
					<label class="key-check"
						><input
							type="checkbox"
							checked={selected.selection.kind === 'all'}
							onchange={(e) => mode(resource.resource_id, e.currentTarget.checked)}
						/>All current permissions</label
					>
					{#if selected.selection.kind === 'subset'}
						{#each resource.capabilities as cap (cap.id)}<label class="key-check"
								><input
									type="checkbox"
									checked={selected.selection.capabilities.includes(cap.id)}
									onchange={(e) =>
										capability(resource.resource_id, cap.id, e.currentTarget.checked)}
								/><span><strong>{cap.key}</strong><small>{cap.meaning}</small></span></label
							>{/each}
					{/if}
				{/if}
			</fieldset>
		{/each}
		<div class="directory-actions">
			<span
				>{choices.length} of 16 resources selected{#if application}
					· one application{/if}</span
			>{#if options.next}<Button
					type="button"
					variant="outline"
					disabled={pending || blocked}
					onclick={() => load(options!.next!)}>Next resources</Button
				>{/if}
		</div>
	{/if}
	{#if choices.length}<ul aria-label="Selected resources">
			{#each choices as choice (choice.resource.resource_id)}<li>
					{choice.resource.application_name} / {choice.resource.resource_name}
					<button
						type="button"
						class="admin-link"
						disabled={pending}
						onclick={() => select(choice.resource, false)}
						aria-label={`Remove ${choice.resource.resource_name}`}>Remove</button
					>
				</li>{/each}
		</ul>{/if}
	<div class="directory-actions">
		<Button type="submit" disabled={pending || loading || blocked || !valid}
			>{pending ? 'Creating…' : 'Create key'}</Button
		><Button type="button" variant="outline" disabled={pending} onclick={cancel}>Cancel</Button>
	</div>
</form>
