<script lang="ts">
	import { useCatalogLocalization } from '$lib/i18n/catalog-context';
	const catalog = useCatalogLocalization();
	import { dateTime as createDateTimeFormatter } from '$lib/i18n/display';
	const dateTime = $derived(createDateTimeFormatter($catalog.locale));
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

<section class="catalog-section credential-section">
	<h3>{$catalog.t('credentials.heading')}</h3>
	<p class="muted">
		{$catalog.t('credentials.help')}
	</p>
	<p class="catalog-help">
		{$catalog.t('credentials.idsHelp')}
	</p>
	<ul class="catalog-options">
		{#each client.secrets ?? [] as secret (secret.id)}<li>
				<span class="catalog-id"
					>{$catalog.t('credentials.record', { id: secret.id })}<br />{secret.expires_ms === null
						? $catalog.t('credentials.current')
						: $catalog.t('credentials.expires', { time: dateTime(secret.expires_ms) })}</span
				><Button
					variant="outline"
					disabled={pending}
					onclick={() => {
						retire = secret.id;
						rotate = false;
					}}>{$catalog.t('credentials.retire')}</Button
				>
			</li>{:else}<li class="catalog-help">
				{$catalog.t('credentials.empty')}
			</li>{/each}
	</ul>
	<Button
		variant="outline"
		disabled={pending}
		onclick={() => {
			rotate = true;
			retire = null;
		}}>{$catalog.t('credentials.rotate')}</Button
	>
	{#if rotate || retire}<form class="catalog-confirm admin-form" onsubmit={submit}>
			<h3>
				{retire ? $catalog.t('credentials.retireTitle') : $catalog.t('credentials.rotateTitle')}
			</h3>
			{#if retire}<p>
					{$catalog.t('credentials.retireHelp')}
				</p>{:else}<label for="secret-overlap">{$catalog.t('credentials.overlap')}</label><input
					id="secret-overlap"
					aria-describedby="secret-overlap-help"
					type="number"
					bind:value={overlap}
					min="0"
					max="300"
					step="1"
					required
					disabled={pending}
				/>
				<p id="secret-overlap-help" class="catalog-help">
					{$catalog.t('credentials.overlapHelp')}
				</p>{/if}
			<div class="modal-actions">
				<Button
					type="button"
					variant="outline"
					disabled={pending}
					onclick={() => {
						rotate = false;
						retire = null;
					}}>{$catalog.t('credentials.cancel')}</Button
				><Button type="submit" disabled={pending}
					>{$catalog.t(
						retire ? 'credentials.confirmRetirement' : 'credentials.confirmRotation'
					)}</Button
				>
			</div>
		</form>{/if}
</section>
