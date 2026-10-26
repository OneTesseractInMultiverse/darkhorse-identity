<script lang="ts">
	import { useCatalogLocalization } from '#lib/i18n/catalog-context.ts';
	import { dateTime } from '#lib/i18n/display.ts';
	import { Button } from '#lib/components/ui/button/index.ts';
	import { logoutApi, type Logout } from '#lib/admin/logout.ts';
	import type { Failure, Item } from '#lib/admin/catalog.ts';
	const catalog = useCatalogLocalization();
	let { client }: { client: Item } = $props();
	let record = $state<Logout | null>(null),
		error = $state<Failure | null>(null);
	let editing = $state(false),
		pending = $state(false),
		uri = $state(''),
		saved = $state(false);
	const api = $derived(logoutApi(fetch, client.application_id!, client.id));
	const formatDate = $derived(dateTime($catalog.locale));
	$effect(() => {
		const current = api;
		let cancelled = false;
		record = null;
		error = null;
		editing = false;
		void current.read().then((result) => {
			if (cancelled) return;
			if (result.kind === 'ready') record = result.data;
			else error = result.kind;
		});
		return () => {
			cancelled = true;
		};
	});
	async function refresh() {
		if (pending) return;
		pending = true;
		error = null;
		const result = await api.read();
		if (result.kind === 'ready') record = result.data;
		else error = result.kind;
		pending = false;
	}
	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (!record || pending) return;
		pending = true;
		error = null;
		saved = false;
		const result = await api.save({
			revision: record.configuration.revision,
			backchannel_logout_uri: uri || null,
			backchannel_logout_session_required: true
		});
		if (result.kind === 'saved') {
			record = { ...record, configuration: result.data };
			editing = false;
			saved = true;
		} else error = result.kind;
		pending = false;
	}
</script>

<section class="catalog-section">
	<h3>{$catalog.t('logout.heading')}</h3>
	<p class="catalog-help">{$catalog.t('logout.help')}</p>
	{#if error}<p role="alert">{$catalog.t(`error.${error}`)}</p>{/if}
	{#if saved}<p role="status">{$catalog.t('message.saved')}</p>{/if}
	{#if record}
		{#if editing}
			<form class="admin-form catalog-confirm" onsubmit={save}>
				<label for="logout-uri">{$catalog.t('logout.uri')}</label>
				<input
					id="logout-uri"
					type="url"
					bind:value={uri}
					maxlength="2048"
					aria-describedby="logout-help"
					disabled={pending}
				/>
				<p id="logout-help" class="catalog-help">{$catalog.t('logout.uriHelp')}</p>
				<p class="catalog-help">{$catalog.t('logout.sessionHelp')}</p>
				<p class="catalog-help">{$catalog.t('logout.changeHelp')}</p>
				<div class="modal-actions">
					<Button
						type="button"
						variant="outline"
						disabled={pending}
						onclick={() => (editing = false)}>{$catalog.t('logout.cancel')}</Button
					>
					<Button type="submit" disabled={pending}>{$catalog.t('logout.save')}</Button>
				</div>
			</form>
		{:else}
			<p class="catalog-id">
				{record.configuration.backchannel_logout_uri ?? $catalog.t('disabled')}
			</p>
			<Button
				variant="outline"
				disabled={pending}
				onclick={() => {
					uri = record!.configuration.backchannel_logout_uri ?? '';
					editing = true;
					saved = false;
				}}>{$catalog.t('logout.configure')}</Button
			>
		{/if}
		<details class="catalog-disclosure">
			<summary>{$catalog.t('logout.deliveries')}</summary>
			<div>
				<p class="catalog-help">{$catalog.t('logout.deliveryHelp')}</p>
				<ul class="catalog-values">
					{#each record.deliveries as delivery (delivery.id)}
						<li>
							{formatDate(delivery.created_ms)} · {$catalog.t(`logout.state.${delivery.state}`)} · {$catalog.t(
								'logout.attempts',
								{ count: delivery.attempts }
							)}
						</li>
					{:else}<li class="catalog-help">{$catalog.t('logout.empty')}</li>{/each}
				</ul>
				<Button variant="outline" disabled={pending} onclick={refresh}
					>{$catalog.t('logout.refresh')}</Button
				>
			</div>
		</details>
	{:else}
		<Button variant="outline" disabled={pending} onclick={refresh}
			>{$catalog.t('logout.refresh')}</Button
		>
	{/if}
</section>
