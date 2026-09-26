<script lang="ts">
	import { useCatalogLocalization } from '$lib/i18n/catalog-context';
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	const catalog = useCatalogLocalization();
	import type { Item } from '$lib/admin/catalog';
	let { client }: { client: Item } = $props();
</script>

<div class="catalog-detail">
	<dl class="catalog-facts">
		<div class="catalog-fact-wide">
			<dt>{$catalog.t('clientId')}</dt>
			<dd class="catalog-id">{client.id}</dd>
			<dd class="catalog-help">
				{$catalog.t('clientIdHelp')}
			</dd>
		</div>
		<div>
			<dt>{$catalog.t('applicationId')}</dt>
			<dd class="catalog-id">{client.application_id}</dd>
			<dd class="catalog-help">{$catalog.t('clientParent')}</dd>
		</div>
		<div>
			<dt>{$language.t('common.status')}</dt>
			<dd>
				<span class="catalog-status" class:inactive={!client.active}
					>{$language.t(client.active ? 'common.active' : 'common.inactive')}</span
				>
			</dd>
			<dd class="catalog-help">
				{$catalog.t('clientActive')}
			</dd>
		</div>
	</dl>
	<section class="catalog-section">
		<h3>{$catalog.t('signInConfiguration')}</h3>
		<p class="catalog-help">{$catalog.t('confidential')}</p>
		<dl class="catalog-settings">
			<div>
				<dt>{$catalog.t('clientAuthentication')}</dt>
				<dd>
					<code>client_secret_basic</code>
					<p class="catalog-help">
						{$catalog.t('clientAuthenticationHelp')}
					</p>
				</dd>
			</div>
			<div>
				<dt>{$catalog.t('callbacks')}</dt>
				<dd>
					<ul class="catalog-values">
						{#each client.redirect_uris ?? [] as uri (uri)}<li class="catalog-id">{uri}</li>{/each}
					</ul>
					<p class="catalog-help">
						{$catalog.t('callbackMatch')}
					</p>
				</dd>
			</div>
			<div>
				<dt>{$catalog.t('refreshTokens')}</dt>
				<dd>
					{client.refresh_tokens ? $catalog.t('allowed') : $catalog.t('disabled')}
					<p class="catalog-help">
						{client.refresh_tokens ? $catalog.t('refreshAllowed') : $catalog.t('refreshDisabled')}
					</p>
				</dd>
			</div>
		</dl>
	</section>
	<details class="catalog-disclosure">
		<summary
			>{$catalog.t('allowances')}
			<span
				>{$catalog.t('allowanceCount', {
					resources: client.resource_ids?.length ?? 0,
					scopes: client.scope_ids?.length ?? 0
				})}</span
			></summary
		>
		<div>
			<p class="catalog-help">
				{$catalog.t('identityAllowances')}
			</p>
			<h4>{$catalog.t('resourceIds')}</h4>
			<ul class="catalog-values">
				{#each client.resource_ids ?? [] as id (id)}<li class="catalog-id">{id}</li>{:else}<li
						class="catalog-help"
					>
						{$catalog.t('noResources')}
					</li>{/each}
			</ul>
			<h4>{$catalog.t('scopeIds')}</h4>
			<ul class="catalog-values">
				{#each client.scope_ids ?? [] as id (id)}<li class="catalog-id">{id}</li>{:else}<li
						class="catalog-help"
					>
						{$catalog.t('noScopes')}
					</li>{/each}
			</ul>
		</div>
	</details>
</div>
