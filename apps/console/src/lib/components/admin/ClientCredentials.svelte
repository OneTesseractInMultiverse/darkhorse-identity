<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import type { Item, Command } from '$lib/admin/catalog';
	let {
		client,
		pending,
		save
	}: { client: Item; pending: boolean; save: (command: Command) => void } = $props();
	let rotate = $state(false),
		retire = $state<string | null>(null),
		overlap = $state(0);
	function command(): Command {
		return retire
			? {
					operation: 'retire_secret',
					application_id: client.application_id,
					client_id: client.id,
					revision: client.revision,
					secret_id: retire
				}
			: {
					operation: 'rotate_secret',
					application_id: client.application_id,
					client_id: client.id,
					revision: client.revision,
					overlap_seconds: overlap
				};
	}
	function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!pending) save(command());
	}
</script>

<h3>Client credentials</h3>
<p class="muted">Client IDs are stable. Secrets are shown only when created or rotated.</p>
<ul class="catalog-options">
	{#each client.secrets ?? [] as secret (secret.id)}<li>
			<span class="catalog-id"
				>{secret.id}<br />{secret.expires_ms === null
					? 'Current'
					: `Expires ${new Date(secret.expires_ms).toISOString()}`}</span
			><Button
				variant="outline"
				disabled={pending}
				onclick={() => {
					retire = secret.id;
					rotate = false;
				}}>Retire secret</Button
			>
		</li>{/each}
</ul>
<Button
	variant="outline"
	disabled={pending}
	onclick={() => {
		rotate = true;
		retire = null;
	}}>Rotate secret</Button
>
{#if rotate || retire}<form class="catalog-confirm admin-form" onsubmit={submit}>
		<h3>{retire ? 'Retire this secret?' : 'Rotate the client secret?'}</h3>
		{#if retire}<p>Clients using this secret will stop authenticating immediately.</p>{:else}<label
				for="secret-overlap">Previous secret overlap (seconds)</label
			><input
				id="secret-overlap"
				type="number"
				bind:value={overlap}
				min="0"
				max="300"
				step="1"
				required
				disabled={pending}
			/>
			<p class="muted">
				Zero retires the previous secret immediately. Any older overlapping secret is retired.
			</p>{/if}
		<div class="modal-actions">
			<Button
				type="button"
				variant="outline"
				disabled={pending}
				onclick={() => {
					rotate = false;
					retire = null;
				}}>Cancel credential change</Button
			><Button type="submit" disabled={pending}>Confirm {retire ? 'retirement' : 'rotation'}</Button
			>
		</div>
	</form>{/if}
