<script lang="ts">
	import { useCatalogLocalization } from '$lib/i18n/catalog-context';
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
	const catalog = useCatalogLocalization();
	import { resolve } from '$app/paths';
	import type { Item } from '$lib/admin/catalog';
	let { application }: { application: Item } = $props();
	const sections = [
		{ path: 'clients', icon: '↗' },
		{ path: 'resources', icon: '◇' },
		{ path: 'scopes', icon: '⌘' },
		{ path: 'roles', icon: '☷' },
		{ path: 'capabilities', icon: '＋' }
	] as const;
</script>

<div class="catalog-detail">
	<dl class="catalog-facts">
		<div class="catalog-fact-wide">
			<dt>{$catalog.t('applicationId')}</dt>
			<dd class="catalog-id">{application.id}</dd>
			<dd class="catalog-help">
				{$catalog.t('applicationIdHelp')}
			</dd>
		</div>
		<div>
			<dt>{$catalog.t('ownerLabel')}</dt>
			<dd>{application.owner_email}</dd>
			<dd class="catalog-help">
				{$catalog.t('ownerContact')}
			</dd>
		</div>
		<div>
			<dt>{$language.t('common.status')}</dt>
			<dd>
				<span class="catalog-status" class:inactive={!application.active}
					>{$language.t(application.active ? 'common.active' : 'common.inactive')}</span
				>
			</dd>
			<dd class="catalog-help">
				{application.active ? $catalog.t('applicationActive') : $catalog.t('applicationInactive')}
			</dd>
		</div>
	</dl>
	<div class="catalog-section-heading">
		<h3>{$catalog.t('configureApplication')}</h3>
		<p class="catalog-help">
			{$catalog.t('configureHelp')}
		</p>
	</div>
	<nav class="catalog-cards" aria-label={$catalog.t('applicationCatalogs')}>
		{#each sections as section (section.path)}
			<a
				class="catalog-card"
				class:catalog-card-primary={section.path === 'clients'}
				href={resolve(
					`/console/${section.path}?application_id=${encodeURIComponent(application.id)}`
				)}
				aria-labelledby={`catalog-card-${section.path}`}
				aria-describedby={`catalog-card-${section.path}-help`}
			>
				<span class="catalog-card-icon" aria-hidden="true">{section.icon}</span>
				<div>
					<span class="catalog-kicker">{$catalog.t(`sectionTag.${section.path}`)}</span>
					<h4 id={`catalog-card-${section.path}`}>
						{$language.t(`console.${section.path}`)} <span aria-hidden="true">↗</span>
					</h4>
					<p id={`catalog-card-${section.path}-help`}>
						{$catalog.t(`intro.${section.path}`)}
					</p>
				</div>
			</a>
		{/each}
	</nav>
	<aside class="catalog-note">
		<strong>{$catalog.t('connectHeading')}</strong>
		<p>
			{$catalog.t('connectHelp')}
		</p>
	</aside>
</div>
