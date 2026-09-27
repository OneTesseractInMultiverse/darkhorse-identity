<script lang="ts">
	import { onMount } from 'svelte';
	import { base } from '$app/paths';
	import { useLocalization } from '$lib/i18n/context';
	import {
		filterOperations,
		filterRouteEntries,
		localizeOperation,
		loadReference,
		operationsFromDocument,
		resolveLocalSchema,
		type ApiOperation,
		type ReferenceState,
		type RouteClassification
	} from '$lib/api-reference';
	import { contract } from '$lib/i18n/contract';
	const language = useLocalization();
	type PanelState = 'loading' | ReferenceState;
	let referenceState = $state<PanelState>('loading');
	let search = $state('');
	let boundarySearch = $state('');
	type CopyStatus = 'copied' | 'failed' | null;
	let copyStatus = $state<CopyStatus>(null);
	let copiedOperation = $state('');
	let loading = false;
	let operations = $derived(
		referenceState !== 'loading' && referenceState.kind === 'ready'
			? operationsFromDocument(referenceState.document).map((operation) =>
					localizeOperation(operation, $language.locale)
				)
			: []
	);
	let visible = $derived(filterOperations(operations, search));
	let groups = $derived(
		Array.from(
			visible.reduce((result, operation) => {
				const key = Array.isArray(operation.value.tags)
					? stringField(operation.value.tags[0]) || 'Other'
					: 'Other';
				const group = result.get(key);
				if (group) group.entries.push(operation);
				else result.set(key, { label: operation.tags[0] ?? key, entries: [operation] });
				return result;
			}, new Map<string, { label: string; entries: ApiOperation[] }>())
		).map(([key, group]) => ({ key, ...group }))
	);
	let boundaryEntries: RouteClassification['entries'] = $derived(
		referenceState !== 'loading' && referenceState.kind === 'ready'
			? filterRouteEntries(referenceState.classification.entries, boundarySearch, boundaryTitle)
			: []
	);
	let boundaries = $derived(groupBoundaries(boundaryEntries));

	function groupBoundaries(
		entries: RouteClassification['entries']
	): Array<[string, RouteClassification['entries']]> {
		const result: Array<[string, RouteClassification['entries']]> = [];
		for (const entry of entries) {
			const group = result.find(([surface]) => surface === entry.surface)?.[1];
			if (group) group.push(entry);
			else result.push([entry.surface, [entry]]);
		}
		return result;
	}

	let active = true;
	function revealAnchor() {
		let id = '';
		try {
			id = decodeURIComponent(window.location.hash.slice(1));
		} catch {
			return;
		}
		requestAnimationFrame(() => {
			const target = document.getElementById(id);
			if (target instanceof HTMLDetailsElement) target.open = true;
		});
	}

	async function refreshReference() {
		if (loading) return;
		loading = true;
		referenceState = 'loading';
		const result = await loadReference(fetch);
		if (active) referenceState = result;
		loading = false;
		revealAnchor();
	}

	onMount(() => {
		active = true;
		void refreshReference();
		window.addEventListener('hashchange', revealAnchor);
		return () => {
			active = false;
			window.removeEventListener('hashchange', revealAnchor);
		};
	});

	function pretty(value: unknown): string {
		if (referenceState === 'loading' || referenceState.kind !== 'ready') return '';
		return JSON.stringify(resolveLocalSchema(value, referenceState.document), null, 2) ?? '';
	}

	function requestExamples(operation: ApiOperation): Array<[string, unknown]> {
		const requestBody = operation.value.requestBody;
		if (!isRecord(requestBody) || !isRecord(requestBody.content)) return [];
		const examples: Array<[string, unknown]> = [];
		for (const content of Object.values(requestBody.content)) {
			if (!isRecord(content) || !isRecord(content.examples)) continue;
			for (const [name, example] of Object.entries(content.examples)) {
				if (isRecord(example) && Object.hasOwn(example, 'value'))
					examples.push([name, example.value]);
			}
		}
		return examples;
	}

	async function copyText(operation: string, text: string) {
		copiedOperation = operation;
		try {
			await navigator.clipboard.writeText(text);
			copyStatus = 'copied';
		} catch {
			copyStatus = 'failed';
		}
	}

	function copyLink(operation: ApiOperation) {
		void copyText(
			operation.operationId,
			`${location.origin}${location.pathname}#operation-${operation.operationId}`
		);
	}

	function groupId(name: string) {
		return `group-${name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
	}

	function boundaryTitle(surface: string) {
		const keys: Record<string, string> = {
			integration: 'apiDocs.integrationSurface',
			first_party_browser: 'apiDocs.browserSurface',
			public_presentation: 'apiDocs.presentationSurface',
			operational: 'apiDocs.operationalSurface',
			static_asset: 'apiDocs.staticSurface'
		};
		const key = keys[surface];
		return key ? $language.t(key as keyof typeof contract) : surface;
	}

	function isRecord(value: unknown): value is Record<string, unknown> {
		return typeof value === 'object' && value !== null && !Array.isArray(value);
	}

	function stringField(value: unknown): string {
		return typeof value === 'string' ? value : '';
	}
</script>

{#if referenceState === 'loading'}
	<p role="status" class="api-notice">{$language.t('apiDocs.loading')}</p>
{:else if referenceState.kind === 'unavailable'}
	<section role="alert" class="api-notice">
		<p>{$language.t('apiDocs.unavailable')}</p>
		<button type="button" onclick={() => void refreshReference()}
			>{$language.t('apiDocs.reload')}</button
		>
	</section>
{:else}
	<div class="api-reference" lang={$language.locale}>
		<header class="api-heading">
			<div>
				<p class="eyebrow">API / REFERENCE</p>
				<h1>{$language.t('apiDocs.title')}</h1>
				<p class="api-intro">{$language.t('apiDocs.intro')}</p>
				<p class="api-release">
					{$language.t('apiDocs.release', { version: referenceState.document.info.version })}
				</p>
			</div>
			<div class="api-downloads" aria-label={$language.t('apiDocs.title')}>
				<!-- Static assets under /reference are served by the Rust router. -->
				<!-- eslint-disable svelte/no-navigation-without-resolve -->
				<a href={`${base}/reference/openapi-v1.json`} download="darkhorse-openapi.json"
					>{$language.t('apiDocs.download')}</a
				>
				<a
					href={`${base}/reference/route-classification-v1.json`}
					download="darkhorse-route-boundary.json">{$language.t('apiDocs.downloadRoutes')}</a
				>
				<!-- eslint-enable svelte/no-navigation-without-resolve -->
			</div>
		</header>

		<section class="api-callout">
			<p class="api-callout-title">{$language.t('apiDocs.notConfigured')}</p>
			<p>{$language.t('apiDocs.noExecutor')}</p>
		</section>

		<section aria-labelledby="api-security-title" class="api-section">
			<h2 id="api-security-title">{$language.t('apiDocs.securityTitle')}</h2>
			<p>{$language.t('apiDocs.securityIntro')}</p>
			<div class="api-security-grid">
				<article>
					<h3>{$language.t('apiDocs.browserTitle')}</h3>
					<p>{$language.t('apiDocs.browserDescription')}</p>
				</article>
				<article>
					<h3>{$language.t('apiDocs.clientTitle')}</h3>
					<p>{$language.t('apiDocs.clientDescription')}</p>
				</article>
				<article>
					<h3>{$language.t('apiDocs.bearerTitle')}</h3>
					<p>{$language.t('apiDocs.bearerDescription')}</p>
				</article>
				<article>
					<h3>{$language.t('apiDocs.resourceTitle')}</h3>
					<p>{$language.t('apiDocs.resourceDescription')}</p>
				</article>
				<article>
					<h3>{$language.t('apiDocs.personalKeyTitle')}</h3>
					<p>{$language.t('apiDocs.personalKeyDescription')}</p>
				</article>
			</div>
		</section>

		<section aria-labelledby="api-walkthrough-title" class="api-section api-walkthrough">
			<h2 id="api-walkthrough-title">{$language.t('apiDocs.walkthroughTitle')}</h2>
			<p>{$language.t('apiDocs.walkthroughIntro')}</p>
			<ol>
				<li>
					<h3>{$language.t('apiDocs.walkthroughRegisterTitle')}</h3>
					<p>{$language.t('apiDocs.walkthroughRegister')}</p>
				</li>
				<li>
					<h3>{$language.t('apiDocs.walkthroughAuthorizeTitle')}</h3>
					<p>{$language.t('apiDocs.walkthroughAuthorize')}</p>
				</li>
				<li>
					<h3>{$language.t('apiDocs.walkthroughCallbackTitle')}</h3>
					<p>{$language.t('apiDocs.walkthroughCallback')}</p>
				</li>
				<li>
					<h3>{$language.t('apiDocs.walkthroughExchangeTitle')}</h3>
					<p>{$language.t('apiDocs.walkthroughExchange')}</p>
				</li>
				<li>
					<h3>{$language.t('apiDocs.walkthroughSessionTitle')}</h3>
					<p>{$language.t('apiDocs.walkthroughSession')}</p>
				</li>
				<li>
					<h3>{$language.t('apiDocs.walkthroughResourceTitle')}</h3>
					<p>{$language.t('apiDocs.walkthroughResource')}</p>
				</li>
				<li>
					<h3>{$language.t('apiDocs.walkthroughRefreshTitle')}</h3>
					<p>{$language.t('apiDocs.walkthroughRefresh')}</p>
				</li>
			</ol>
			<p class="api-callout api-walkthrough-note">{$language.t('apiDocs.logoutNote')}</p>
		</section>

		<section id="operations" aria-labelledby="api-operations-title" class="api-section">
			<div class="api-section-heading">
				<div>
					<h2 id="api-operations-title">{$language.t('apiDocs.operationGroups')}</h2>
					<p>{$language.t('apiDocs.protocolOnly')}</p>
				</div>
				<p class="api-result-count" role="status" aria-live="polite">
					{$language.t('apiDocs.results', { count: visible.length })}
				</p>
			</div>
			<label class="api-search">
				<span>{$language.t('apiDocs.search')}</span>
				<input bind:value={search} maxlength="128" type="search" autocomplete="off" />
				<small>{$language.t('apiDocs.searchHelp')}</small>
			</label>
			{#if visible.length === 0}
				<p role="status" class="api-notice">{$language.t('apiDocs.noResults')}</p>
			{:else}
				<nav class="api-group-nav" aria-label={$language.t('apiDocs.operationGroups')}>
					{#each groups as group (group.key)}
						<a href="#{groupId(group.key)}">{group.label}<span>{group.entries.length}</span></a>
					{/each}
				</nav>
				{#each groups as group (group.key)}
					<section class="api-operation-group" id={groupId(group.key)} aria-label={group.label}>
						<h3>{group.label}</h3>
						{#each group.entries as operation (operation.operationId)}
							<div class="api-operation-row">
								<details id="operation-{operation.operationId}" class="api-operation">
									<summary>
										<span class="api-method method-{operation.method.toLowerCase()}"
											>{operation.method}</span
										>
										<code>{operation.path}</code>
										<span class="api-operation-summary">{operation.summary}</span>
										<code class="api-operation-id">{operation.operationId}</code>
									</summary>
									<div class="api-operation-body">
										<p>{operation.description}</p>
										{#if operation.value.security}
											<h4>{$language.t('apiDocs.authentication')}</h4>
											<pre><code>{pretty(operation.value.security)}</code></pre>
										{/if}
										{#if operation.value.parameters}
											<h4>{$language.t('apiDocs.parameters')}</h4>
											<pre><code>{pretty(operation.value.parameters)}</code></pre>
										{/if}
										{#if operation.value.requestBody}
											<h4>{$language.t('apiDocs.request')}</h4>
											<pre><code>{pretty(operation.value.requestBody)}</code></pre>
											{#each requestExamples(operation) as [name, example] (name)}
												<div class="api-example-action">
													<span>{name}</span>
													<button
														type="button"
														onclick={() =>
															copyText(
																operation.operationId,
																JSON.stringify(example, null, 2) ?? ''
															)}
													>
														{$language.t('apiDocs.copyExample', {
															operation: operation.operationId,
															example: name
														})}
													</button>
												</div>
											{/each}
										{/if}
										<h4>{$language.t('apiDocs.responses')}</h4>
										<pre><code>{pretty(operation.value.responses)}</code></pre>
									</div>
								</details>
								<button class="api-permalink" type="button" onclick={() => copyLink(operation)}>
									{$language.t('apiDocs.copyLink', { operation: operation.operationId })}
								</button>
							</div>
						{/each}
					</section>
				{/each}
			{/if}
		</section>

		{#if copyStatus}
			<p role="status" class="api-copy-status">
				{copyStatus === 'copied'
					? $language.t('apiDocs.copied')
					: $language.t('apiDocs.copyFailed')}
			</p>
		{/if}

		<section class="api-section api-boundaries" aria-labelledby="api-boundaries-title">
			<h2 id="api-boundaries-title">{$language.t('apiDocs.boundaries')}</h2>
			<p>{$language.t('apiDocs.boundaryIntro')}</p>
			<label class="api-search">
				<span>{$language.t('apiDocs.routeSearch')}</span>
				<input bind:value={boundarySearch} maxlength="128" type="search" autocomplete="off" />
				<small>{$language.t('apiDocs.routeSearchHelp')}</small>
			</label>
			<p class="api-result-count" role="status" aria-live="polite">
				{$language.t('apiDocs.routeResults', { count: boundaryEntries.length })}
			</p>
			{#if boundaryEntries.length === 0}
				<p role="status" class="api-notice">{$language.t('apiDocs.noRouteResults')}</p>
			{:else}
				{#each boundaries as [surface, entries] (surface)}
					<details>
						<summary>{boundaryTitle(surface)} <span>{entries.length}</span></summary>
						<ul>
							{#each entries as entry (`${entry.method}-${entry.path}-${entry.source}-${entry.handler}`)}
								<li class="api-boundary-entry">
									<code>{entry.method} {entry.path}</code><span>{entry.rationale}</span>
								</li>
							{/each}
						</ul>
					</details>
				{/each}
			{/if}
		</section>
	</div>
{/if}

{#if copyStatus}
	<span class="sr-only" aria-live="polite">{copiedOperation}</span>
{/if}

<style>
	.api-reference {
		width: min(100%, 1120px);
		display: grid;
		gap: 30px;
	}
	.api-heading,
	.api-section-heading {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 24px;
	}
	.api-heading h1 {
		margin: 10px 0;
		font-size: clamp(30px, 4vw, 42px);
		letter-spacing: -0.04em;
	}
	.api-intro,
	.api-section > p,
	.api-section-heading p,
	.api-callout p {
		color: var(--color-muted-foreground);
		line-height: 1.65;
		max-width: 78ch;
	}
	.api-release,
	.api-result-count {
		color: var(--color-primary);
		font:
			12px ui-monospace,
			monospace;
	}
	.api-downloads {
		display: grid;
		gap: 10px;
		min-width: 230px;
	}
	.api-downloads a,
	.api-example-action button,
	.api-permalink,
	.api-notice button {
		border: 1px solid var(--color-border);
		border-radius: 9px;
		color: var(--color-foreground);
		padding: 10px 13px;
		background: #101914;
		text-decoration: none;
		font-size: 13px;
	}
	.api-downloads a:hover,
	.api-example-action button:hover,
	.api-permalink:hover,
	.api-notice button:hover {
		border-color: var(--color-primary);
		color: var(--color-primary);
	}
	.api-callout,
	.api-security-grid article,
	.api-operation,
	.api-boundaries > details,
	.api-notice {
		border: 1px solid var(--color-border);
		border-radius: 14px;
		background: #101914b8;
	}
	.api-callout {
		padding: 18px 22px;
		border-color: #91e4b44a;
	}
	.api-callout-title {
		margin: 0;
		color: var(--color-foreground) !important;
		font-size: 14px;
		font-weight: 600;
	}
	.api-callout p {
		margin: 10px 0 0;
	}
	.api-section {
		display: grid;
		gap: 14px;
		min-width: 0;
	}
	.api-section h2 {
		margin: 0;
		font-size: 22px;
	}
	.api-security-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 270px), 1fr));
		gap: 12px;
	}
	.api-security-grid article {
		padding: 18px;
	}
	.api-security-grid h3,
	.api-operation-group h3 {
		margin: 0;
		font-size: 15px;
	}
	.api-security-grid p {
		color: var(--color-muted-foreground);
		font-size: 13px;
		line-height: 1.6;
		margin-bottom: 0;
	}
	.api-walkthrough ol {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 12px;
	}
	.api-walkthrough li {
		min-width: 0;
		padding: 18px;
		border: 1px solid var(--color-border);
		border-radius: 14px;
		background: #101914b8;
	}
	.api-walkthrough h3 {
		margin: 0 0 8px;
		font-size: 15px;
	}
	.api-walkthrough li p,
	.api-walkthrough-note {
		margin: 0;
		color: var(--color-muted-foreground);
		font-size: 13px;
		line-height: 1.6;
	}
	.api-walkthrough-note {
		padding: 14px 18px;
	}
	.api-section-heading {
		align-items: end;
	}
	.api-section-heading p {
		margin: 8px 0 0;
	}
	.api-search {
		display: grid;
		gap: 8px;
		max-width: 720px;
	}
	.api-search span {
		font-size: 13px;
		font-weight: 600;
	}
	.api-search input {
		width: 100%;
		border: 1px solid var(--color-border);
		border-radius: 10px;
		padding: 12px 14px;
		background: #0b1510;
		color: var(--color-foreground);
	}
	.api-search small {
		color: var(--color-muted-foreground);
		line-height: 1.5;
	}
	.api-group-nav {
		display: flex;
		flex-wrap: wrap;
		gap: 9px;
	}
	.api-group-nav a {
		display: inline-flex;
		gap: 9px;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		padding: 8px 12px;
		color: var(--color-foreground);
		text-decoration: none;
		font-size: 12px;
	}
	.api-group-nav span,
	.api-boundaries summary span {
		color: var(--color-primary);
		font:
			11px ui-monospace,
			monospace;
	}
	.api-operation-group {
		display: grid;
		gap: 10px;
		scroll-margin-top: 24px;
	}
	.api-operation-row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: start;
		gap: 8px;
	}
	.api-operation {
		min-width: 0;
		scroll-margin-top: 24px;
		overflow: hidden;
	}
	.api-operation summary,
	.api-boundaries summary {
		cursor: pointer;
		list-style: none;
	}
	.api-operation summary::-webkit-details-marker,
	.api-boundaries summary::-webkit-details-marker {
		display: none;
	}
	.api-operation summary {
		min-height: 54px;
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 12px;
		padding: 12px 15px;
	}
	.api-operation summary::after,
	.api-boundaries summary::after {
		content: '+';
		color: var(--color-primary);
		margin-left: auto;
		font:
			18px ui-monospace,
			monospace;
	}
	.api-operation[open] summary::after,
	.api-boundaries > details[open] summary::after {
		content: '−';
	}
	.api-operation summary code,
	.api-boundaries code {
		font:
			12px ui-monospace,
			monospace;
		overflow-wrap: anywhere;
	}
	.api-method {
		min-width: 52px;
		color: var(--color-primary);
		font:
			11px ui-monospace,
			monospace;
		letter-spacing: 0.05em;
	}
	.method-post {
		color: #c6a2ff;
	}
	.api-operation-summary {
		font-size: 13px;
	}
	.api-operation-id {
		color: var(--color-muted-foreground);
		margin-left: auto;
	}
	.api-operation-body {
		padding: 0 16px 18px;
		border-top: 1px solid #2c3b3266;
	}
	.api-operation-body p {
		color: var(--color-muted-foreground);
		line-height: 1.65;
		white-space: pre-line;
	}
	.api-operation-body h4 {
		margin: 20px 0 8px;
		font:
			12px ui-monospace,
			monospace;
		letter-spacing: 0.04em;
		color: var(--color-primary);
	}
	.api-operation-body pre {
		max-height: 440px;
		overflow: auto;
		margin: 0;
		padding: 14px;
		border-radius: 9px;
		background: #090d0c;
		color: #dce8e0;
		font-size: 12px;
		line-height: 1.6;
		tab-size: 2;
	}
	.api-operation-body pre code {
		white-space: pre;
		font-family: ui-monospace, monospace;
	}
	.api-example-action {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 12px;
		margin-top: 8px;
		color: var(--color-muted-foreground);
		font-size: 12px;
	}
	.api-permalink {
		white-space: nowrap;
		font-size: 11px;
	}
	.api-copy-status,
	.api-notice {
		padding: 12px 16px;
		color: var(--color-muted-foreground);
		line-height: 1.5;
	}
	.api-copy-status {
		border-left: 2px solid var(--color-primary);
	}
	.api-boundaries > details {
		overflow: hidden;
	}
	.api-boundaries summary {
		display: flex;
		gap: 12px;
		align-items: center;
		padding: 15px 18px;
	}
	.api-boundaries ul {
		display: grid;
		gap: 8px;
		margin: 0;
		padding: 0 18px 18px 36px;
	}
	.api-boundaries li {
		padding-left: 4px;
	}
	.api-boundaries li code {
		display: block;
		color: var(--color-primary);
	}
	.api-boundaries li span {
		display: block;
		margin-top: 4px;
		color: var(--color-muted-foreground);
		font-size: 12px;
		line-height: 1.5;
	}
	.api-notice {
		padding: 18px;
	}
	@media (max-width: 720px) {
		.api-heading,
		.api-section-heading {
			flex-direction: column;
			align-items: stretch;
		}
		.api-downloads {
			min-width: 0;
			grid-template-columns: 1fr;
		}
		.api-walkthrough ol {
			grid-template-columns: 1fr;
		}
		.api-result-count {
			margin: 0;
		}
		.api-operation-row {
			grid-template-columns: 1fr;
		}
		.api-permalink {
			justify-self: end;
		}
		.api-operation-id {
			width: 100%;
			margin-left: 64px;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		:global(*),
		:global(*::before),
		:global(*::after) {
			scroll-behavior: auto !important;
			animation-duration: 0.01ms !important;
			transition-duration: 0.01ms !important;
		}
	}
</style>
