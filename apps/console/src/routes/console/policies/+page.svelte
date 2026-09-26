<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { useLocalization } from '$lib/i18n/context';
	import { useCatalogLocalization } from '$lib/i18n/catalog-context';
	import ConsoleShell from '$lib/components/admin/ConsoleShell.svelte';
	import PolicyFlow from '$lib/components/admin/PolicyFlow.svelte';
	import { catalogApi } from '$lib/admin/catalog';
	import { reference } from '$lib/admin/catalog-decode';
	import { readPolicyMap } from '$lib/admin/policy-map-api';
	import {
		canRenderPolicyFlow,
		MAX_POLICY_FLOW_EDGES,
		MAX_POLICY_FLOW_NODES
	} from '$lib/admin/policy-flow';
	import {
		visiblePolicyIds,
		type PolicyMap,
		type PolicyNode,
		type PolicyNodeType,
		type Relationship,
		type PolicyLabels
	} from '$lib/admin/policy-map';
	const language = useLocalization();
	const catalog = useCatalogLocalization();
	const pageFetch: typeof fetch = (input, init) => fetch(input, init);
	const applicationsApi = catalogApi(pageFetch);
	type Failure = 'signed-out' | 'forbidden' | 'too-large' | 'unavailable' | 'reference';
	type ManagementPath = `${
		| '/console/applications'
		| '/console/capabilities'
		| '/console/roles'
		| '/console/resources'
		| '/console/scopes'}?${string}`;
	let mounted = $state(false);
	let selectedId = $state('');
	let candidateId = $state('');
	let graph = $state<PolicyMap | null>(null);
	let busy = $state(false);
	let failure = $state<Failure | null>(null);
	let applications = $state<{ id: string; name: string }[]>([]);
	let applicationsBusy = $state(false);
	let applicationsFailure = $state(false);
	let applicationSearch = $state('');
	let applicationAfter = $state<string | undefined>();
	let search = $state('');
	let relationship = $state<Relationship | ''>('');
	let focusId = $state<string | null>(null);
	let selectedNodeId = $state<string | null>(null);
	let view = $state<'graph' | 'list'>('graph');
	let graphKey = $state(0);
	let edgePage = $state(0);
	let itemPage = $state(0);
	let entity = $state<PolicyNodeType | ''>('');
	let loadedAt = $state('');
	let controller: AbortController | undefined;
	let previousId: string | undefined;
	let applicationRequest = 0;
	const relationshipOptions: Relationship[] = [
		'application_role',
		'application_capability',
		'application_resource',
		'role_capability',
		'resource_capability',
		'resource_scope',
		'scope_capability'
	];
	const entityOptions: PolicyNodeType[] = [
		'application',
		'role',
		'capability',
		'resource',
		'scope'
	];
	const labels = $derived<PolicyLabels>({
		application: $catalog.t('policy.type.application'),
		role: $catalog.t('policy.type.role'),
		capability: $catalog.t('policy.type.capability'),
		resource: $catalog.t('policy.type.resource'),
		scope: $catalog.t('policy.type.scope'),
		application_role: $catalog.t('policy.relation.application_role'),
		application_capability: $catalog.t('policy.relation.application_capability'),
		application_resource: $catalog.t('policy.relation.application_resource'),
		role_capability: $catalog.t('policy.relation.role_capability'),
		resource_capability: $catalog.t('policy.relation.resource_capability'),
		resource_scope: $catalog.t('policy.relation.resource_scope'),
		scope_capability: $catalog.t('policy.relation.scope_capability')
	});
	const flowLabels = $derived({
		region: $catalog.t('policy.graphRegion'),
		controls: $catalog.t('policy.graphControls'),
		zoomIn: $catalog.t('policy.zoomIn'),
		zoomOut: $catalog.t('policy.zoomOut'),
		fit: $catalog.t('policy.fitView'),
		toggle: $catalog.t('policy.toggleInteraction'),
		minimap: $catalog.t('policy.minimap'),
		node: $catalog.t('policy.nodeA11y'),
		edge: $catalog.t('policy.edgeA11y')
	});
	let visible = $derived(
		graph
			? visiblePolicyIds(graph, { search, relationship, entity, focusId })
			: { nodeIds: new Set<string>(), edgeIds: new Set<string>() }
	);
	let graphWithinInteractiveLimit = $derived(
		canRenderPolicyFlow(visible.nodeIds.size, visible.edgeIds.size)
	);
	let nodeById = $derived(new Map(graph?.nodes.map((node) => [node.id, node]) ?? []));
	let selectedNode = $derived(nodeById.get(selectedNodeId ?? '') ?? null);
	let selectedLinks = $derived(
		graph && selectedNode
			? graph.edges
					.filter((edge) => edge.source === selectedNode.id || edge.target === selectedNode.id)
					.map((edge) => ({
						relationship: edge.relationship,
						other: nodeById.get(edge.source === selectedNode.id ? edge.target : edge.source)!
					}))
			: []
	);
	let filteredEdges = $derived(
		graph ? graph.edges.filter((edge) => visible.edgeIds.has(edge.id)) : []
	);
	let filteredNodes = $derived(
		graph ? graph.nodes.filter((node) => visible.nodeIds.has(node.id)) : []
	);
	let pageEdges = $derived(filteredEdges.slice(edgePage * 50, edgePage * 50 + 50));
	let pageNodes = $derived(filteredNodes.slice(itemPage * 50, itemPage * 50 + 50));
	onMount(() => {
		mounted = true;
		void loadApplications();
		return () => {
			mounted = false;
			controller?.abort();
			graph = null;
		};
	});
	$effect(() => {
		const search = page.url.search || window.location.search;
		const id = new URLSearchParams(search).get('application_id') ?? '';
		if (!mounted || id === previousId) return;
		previousId = id;
		selectedId = id;
		void loadGraph(id);
	});
	async function loadApplications(after?: string) {
		const request = ++applicationRequest;
		applicationsBusy = true;
		applicationsFailure = false;
		const result = await applicationsApi.list('applications', {
			search: applicationSearch,
			after
		});
		if (!mounted || request !== applicationRequest) return;
		applicationsBusy = false;
		if (result.kind === 'ready') {
			const found = result.data.items
				.filter((item) => item.kind === 'application')
				.map((item) => ({ id: item.id, name: item.name }));
			applications = after ? [...applications, ...found] : found;
			applicationAfter = result.data.next ?? undefined;
		} else applicationsFailure = true;
	}
	async function loadGraph(id: string) {
		controller?.abort();
		controller = undefined;
		graph = null;
		loadedAt = '';
		edgePage = 0;
		failure = null;
		focusId = null;
		selectedNodeId = null;
		itemPage = 0;
		if (!id) {
			busy = false;
			return;
		}
		if (!reference(id)) {
			failure = 'reference';
			busy = false;
			return;
		}
		const request = new AbortController();
		controller = request;
		busy = true;
		const result = await readPolicyMap(pageFetch, id, request.signal);
		if (!mounted || request.signal.aborted) return;
		busy = false;
		controller = undefined;
		if (result.kind === 'ready') {
			graph = result.data;
			loadedAt = new Date().toISOString();
			if (!applications.some((application) => application.id === result.data.application.id))
				applications = [...applications, result.data.application];
			candidateId = result.data.application.id;
		} else if (result.kind !== 'aborted') failure = result.kind;
	}
	function chooseApplication(event: SubmitEvent) {
		event.preventDefault();
		if (!reference(candidateId)) return;
		void goto(resolve(`/console/policies?application_id=${encodeURIComponent(candidateId)}`));
	}
	function searchApplications(event: SubmitEvent) {
		event.preventDefault();
		applications = [];
		applicationAfter = undefined;
		void loadApplications();
	}
	function nextApplications() {
		if (applicationAfter && !applicationsBusy) void loadApplications(applicationAfter);
	}
	function selectNode(id: string | null) {
		selectedNodeId = id;
	}
	function resetGraph() {
		search = '';
		relationship = '';
		entity = '';
		focusId = null;
		edgePage = 0;
		itemPage = 0;
		graphKey += 1;
	}
	function nodeName(node: PolicyNode | undefined): string {
		return node?.name ?? $language.t('common.unspecified');
	}
	function describeFailure(kind: Failure): string {
		return $catalog.t(`policy.error.${kind}`);
	}
	function nodeGlyph(type: PolicyNodeType): string {
		return { application: '◉', role: '◈', capability: '◆', resource: '▤', scope: '◌' }[type];
	}
	function managementHref(node: PolicyNode): ManagementPath {
		const collection: 'applications' | 'capabilities' | 'roles' | 'resources' | 'scopes' =
			node.type === 'application'
				? 'applications'
				: node.type === 'capability'
					? 'capabilities'
					: node.type === 'role'
						? 'roles'
						: node.type === 'resource'
							? 'resources'
							: 'scopes';
		const query = new URLSearchParams({
			application_id: graph!.application.id,
			item_kind: node.type,
			item_id: node.identifier,
			...(node.type === 'scope' ? { resource_id: node.resource_id!.slice('resource:'.length) } : {})
		});
		return `/console/${collection}?${query}` as ManagementPath;
	}
</script>

<svelte:head>
	<title>{$language.t('console.pageTitle', { name: $language.t('console.policyMap') })}</title>
	<meta name="description" content={$catalog.t('policy.description')} />
</svelte:head>

<ConsoleShell active="policies">
	<section class="policy-page" aria-labelledby="policy-title">
		<header class="policy-heading">
			<p class="policy-eyebrow">{$catalog.t('policy.eyebrow')}</p>
			<h1 id="policy-title">{$catalog.t('policy.title')}</h1>
			<p class="muted">{$catalog.t('policy.description')}</p>
		</header>

		<section class="policy-picker" aria-labelledby="policy-picker-title">
			<div>
				<h2 id="policy-picker-title">{$catalog.t('policy.application')}</h2>
				<p class="muted">{$catalog.t('policy.applicationHelp')}</p>
			</div>
			<form class="policy-application-search" onsubmit={searchApplications}>
				<label for="application-search">{$catalog.t('policy.searchApplications')}</label>
				<div class="policy-search-row">
					<input
						id="application-search"
						bind:value={applicationSearch}
						maxlength="200"
						autocomplete="off"
						placeholder={$catalog.t('policy.applicationPlaceholder')}
					/>
					<button type="submit" class="policy-button" disabled={applicationsBusy}>
						{$catalog.t('policy.findApplications')}
					</button>
				</div>
			</form>
			<form class="policy-application-select" onsubmit={chooseApplication}>
				<label for="application-choice">{$catalog.t('policy.chooseApplication')}</label>
				<select id="application-choice" bind:value={candidateId}>
					<option value="">{$catalog.t('policy.applicationPlaceholder')}</option>
					{#each applications as application (application.id)}
						<option value={application.id}>{application.name}</option>
					{/each}
				</select>
				<button
					class="policy-button policy-button-primary"
					disabled={!reference(candidateId) || busy}
				>
					{$catalog.t('policy.load')}
				</button>
				{#if applicationAfter}
					<button
						type="button"
						class="policy-button"
						onclick={nextApplications}
						disabled={applicationsBusy}
					>
						{$catalog.t('policy.nextApplications')}
					</button>
				{/if}
			</form>
			{#if applicationsBusy}<p class="policy-inline-status" role="status">
					{$catalog.t('policy.loadingApplications')}
				</p>{/if}
			{#if applicationsFailure}<p class="policy-error" role="alert">
					{$catalog.t('policy.applicationError')}
				</p>{/if}
			{#if !applicationsBusy && applications.length === 0}<p class="policy-inline-status">
					{$catalog.t('policy.noApplications')}
				</p>{/if}
		</section>

		{#if busy}
			<div class="policy-state" role="status">
				<span class="policy-pulse" aria-hidden="true"></span>{$catalog.t('policy.loading')}
			</div>
		{:else if failure}
			<div class="policy-state policy-error" role="alert">
				<p>{describeFailure(failure)}</p>
				{#if failure === 'signed-out'}<a href={resolve('/')}>{$catalog.t('policy.signIn')}</a>{/if}
			</div>
		{:else if !graph}
			<div class="policy-state"><p>{$catalog.t('policy.graphEmpty')}</p></div>
		{:else}
			<section class="policy-snapshot" aria-label={$catalog.t('policy.snapshot')}>
				<div>
					<p class="policy-eyebrow">{$catalog.t('policy.currentApplication')}</p>
					<h2>{graph.application.name}</h2>
					<p class="muted">
						{$catalog.t('policy.revision', { revision: graph.policy_revision })}
						<span aria-hidden="true"> · </span>
						{graph.nodes.length}
						{$catalog.t('policy.nodes')}
						<span aria-hidden="true"> · </span>
						{graph.edges.length}
						{$catalog.t('policy.relationships')}
					</p>
					<p class="policy-freshness">
						{$catalog.t('policy.loadedAt', { time: loadedAt })}
					</p>
				</div>
				<p class="policy-complete">
					<span aria-hidden="true">✓</span>
					{$catalog.t('policy.completeNotice')}
				</p>
			</section>

			<div class="policy-legend" aria-label={$catalog.t('policy.legend')}>
				{#each entityOptions as kind (kind)}
					<span class={`policy-legend-${kind}`}
						><b aria-hidden="true">{nodeGlyph(kind)}</b>{labels[kind]}</span
					>
				{/each}
			</div>

			<section class="policy-tools" aria-label={$catalog.t('policy.filters')}>
				<label class="policy-filter-search" for="policy-search">
					<span>{$catalog.t('policy.search')}</span>
					<input
						id="policy-search"
						bind:value={search}
						maxlength="200"
						autocomplete="off"
						oninput={() => {
							edgePage = 0;
							itemPage = 0;
						}}
					/>
				</label>
				<label for="entity-filter">
					<span>{$catalog.t('policy.entityFilter')}</span>
					<select
						id="entity-filter"
						bind:value={entity}
						onchange={() => {
							edgePage = 0;
							itemPage = 0;
						}}
					>
						<option value="">{$catalog.t('policy.allEntities')}</option>
						{#each entityOptions as kind (kind)}<option value={kind}>{labels[kind]}</option>{/each}
					</select>
				</label>
				<label for="relationship-filter">
					<span>{$catalog.t('policy.relationshipFilter')}</span>
					<select
						id="relationship-filter"
						bind:value={relationship}
						onchange={() => {
							edgePage = 0;
							itemPage = 0;
						}}
					>
						<option value="">{$catalog.t('policy.allRelationships')}</option>
						{#each relationshipOptions as option (option)}<option value={option}
								>{labels[option]}</option
							>{/each}
					</select>
				</label>
				<div class="policy-view-toggle" role="group" aria-label={$catalog.t('policy.view')}>
					<button
						type="button"
						class:active={view === 'graph'}
						aria-pressed={view === 'graph'}
						onclick={() => (view = 'graph')}
					>
						{$catalog.t('policy.viewGraph')}
					</button>
					<button
						type="button"
						class:active={view === 'list'}
						aria-pressed={view === 'list'}
						onclick={() => (view = 'list')}
					>
						{$catalog.t('policy.viewList')}
					</button>
				</div>
				<button type="button" class="policy-button" onclick={resetGraph}
					>{$catalog.t('policy.resetView')}</button
				>
				<button
					type="button"
					class="policy-button"
					onclick={() => void loadGraph(selectedId)}
					disabled={busy}
				>
					{$catalog.t('policy.refresh')}
				</button>
			</section>

			{#if focusId}
				<p class="policy-focus-note" role="status">
					{$catalog.t('policy.focusedOn', {
						name: nodeName(graph.nodes.find((node) => node.id === focusId))
					})}
					<button
						type="button"
						onclick={() => {
							focusId = null;
							edgePage = 0;
							itemPage = 0;
						}}>{$catalog.t('policy.clearFocus')}</button
					>
				</p>
			{/if}
			{#if search.trim() || relationship || entity || focusId}
				<p class="policy-focus-note" role="status">
					{$catalog.t('policy.filteredNotice', {
						visible: visible.edgeIds.size,
						total: graph.edges.length
					})}
				</p>
			{/if}

			<div class="policy-workspace">
				<div class="policy-main-view">
					{#if visible.nodeIds.size === 0}
						<div class="policy-state">{$catalog.t('policy.noMatches')}</div>
					{:else if view === 'graph' && !graphWithinInteractiveLimit}
						<div class="policy-state policy-graph-limit" role="status">
							<h3>{$catalog.t('policy.graphLimitTitle')}</h3>
							<p>
								{$catalog.t('policy.graphLimitDescription', {
									nodes: visible.nodeIds.size,
									edges: visible.edgeIds.size,
									maxNodes: MAX_POLICY_FLOW_NODES,
									maxEdges: MAX_POLICY_FLOW_EDGES
								})}
							</p>
							<button type="button" class="policy-button" onclick={() => (view = 'list')}>
								{$catalog.t('policy.viewList')}
							</button>
						</div>
					{:else if view === 'graph'}
						{#key graphKey}
							<PolicyFlow
								{graph}
								{labels}
								visibleNodes={visible.nodeIds}
								visibleEdges={visible.edgeIds}
								selectedId={selectedNodeId}
								{flowLabels}
								onSelect={selectNode}
							/>
						{/key}
					{:else}
						<div class="policy-table-region">
							<div class="policy-table-scroll">
								<table class="policy-table">
									<caption
										>{$catalog.t('policy.itemsCaption', { count: filteredNodes.length })}</caption
									>
									<thead
										><tr
											><th scope="col">{$catalog.t('policy.entityFilter')}</th><th scope="col"
												>{$catalog.t('name')}</th
											><th scope="col">{$catalog.t('policy.identifier')}</th></tr
										></thead
									>
									<tbody>
										{#each pageNodes as node (node.id)}
											<tr>
												<td>{nodeGlyph(node.type)} {labels[node.type]}</td>
												<td
													><button
														class="policy-node-link"
														type="button"
														onclick={() => selectNode(node.id)}>{node.name}</button
													></td
												>
												<td class="policy-identifier">{node.identifier}</td>
											</tr>
										{/each}
									</tbody>
								</table>
							</div>
							<nav class="policy-table-pagination" aria-label={$catalog.t('policy.entityFilter')}>
								<button
									type="button"
									class="policy-button"
									disabled={itemPage === 0}
									onclick={() => (itemPage -= 1)}>{$catalog.t('policy.previousItems')}</button
								>
								<span aria-live="polite"
									>{$catalog.t('policy.itemRange', {
										from: filteredNodes.length === 0 ? 0 : itemPage * 50 + 1,
										to: Math.min((itemPage + 1) * 50, filteredNodes.length),
										total: filteredNodes.length
									})}</span
								>
								<button
									type="button"
									class="policy-button"
									disabled={(itemPage + 1) * 50 >= filteredNodes.length}
									onclick={() => (itemPage += 1)}>{$catalog.t('policy.nextItems')}</button
								>
							</nav>
							{#if visible.edgeIds.size === 0}
								<div class="policy-state">{$catalog.t('policy.noRelationships')}</div>
							{:else}
								<div class="policy-table-scroll">
									<table class="policy-table">
										<caption
											>{$catalog.t('policy.listCaption', { count: visible.edgeIds.size })}</caption
										>
										<thead
											><tr
												><th scope="col">{$catalog.t('policy.source')}</th><th scope="col"
													>{$catalog.t('policy.relationship')}</th
												><th scope="col">{$catalog.t('policy.target')}</th></tr
											></thead
										>
										<tbody>
											{#each pageEdges as edge (edge.id)}
												<tr>
													<td
														><button
															class="policy-node-link"
															type="button"
															onclick={() => selectNode(edge.source)}
															>{nodeName(nodeById.get(edge.source))}</button
														></td
													>
													<td>{labels[edge.relationship]}</td>
													<td
														><button
															class="policy-node-link"
															type="button"
															onclick={() => selectNode(edge.target)}
															>{nodeName(nodeById.get(edge.target))}</button
														></td
													>
												</tr>
											{/each}
										</tbody>
									</table>
								</div>
								<nav class="policy-table-pagination" aria-label={$catalog.t('policy.relationship')}>
									<button
										type="button"
										class="policy-button"
										disabled={edgePage === 0}
										onclick={() => (edgePage -= 1)}
									>
										{$catalog.t('policy.previousRelationships')}
									</button>
									<span aria-live="polite">
										{$catalog.t('policy.relationshipRange', {
											from: filteredEdges.length === 0 ? 0 : edgePage * 50 + 1,
											to: Math.min((edgePage + 1) * 50, filteredEdges.length),
											total: filteredEdges.length
										})}
									</span>
									<button
										type="button"
										class="policy-button"
										disabled={(edgePage + 1) * 50 >= filteredEdges.length}
										onclick={() => (edgePage += 1)}
									>
										{$catalog.t('policy.nextRelationships')}
									</button>
								</nav>
							{/if}
						</div>
					{/if}
				</div>

				<aside class="policy-inspector" aria-labelledby="policy-inspector-title" aria-live="polite">
					<p class="policy-eyebrow">{$catalog.t('policy.inspector')}</p>
					{#if selectedNode}
						<h3 id="policy-inspector-title">{labels[selectedNode.type]}</h3>
						<p class="policy-node-name">{selectedNode.name}</p>
						<dl>
							<div>
								<dt>{$catalog.t('policy.identifier')}</dt>
								<dd class="policy-identifier">{selectedNode.identifier}</dd>
							</div>
							{#if selectedNode.type === 'capability'}<div>
									<dt>{$catalog.t('policy.permissionKey')}</dt>
									<dd>{selectedNode.key}</dd>
								</div>
								<div>
									<dt>{$catalog.t('policy.meaning')}</dt>
									<dd>{selectedNode.meaning}</dd>
								</div>
								<div>
									<dt>{$language.t('common.status')}</dt>
									<dd>{$language.t(selectedNode.retired ? 'common.inactive' : 'common.active')}</dd>
								</div>{/if}
							{#if selectedNode.type === 'resource'}<div>
									<dt>{$catalog.t('policy.audience')}</dt>
									<dd class="policy-identifier">{selectedNode.audience}</dd>
								</div>{/if}
							{#if selectedNode.type === 'scope'}<div>
									<dt>{$catalog.t('policy.parentResource')}</dt>
									<dd>
										{nodeName(
											selectedNode.resource_id ? nodeById.get(selectedNode.resource_id) : undefined
										)}
									</dd>
								</div>{/if}
							{#if selectedNode.type === 'application'}<div>
									<dt>{$language.t('common.status')}</dt>
									<dd>{$language.t(selectedNode.active ? 'common.active' : 'common.inactive')}</dd>
								</div>{/if}
						</dl>
						<button type="button" class="policy-button" onclick={() => (focusId = selectedNode!.id)}
							>{$catalog.t('policy.focusDirect')}</button
						>
						<a class="policy-management-link" href={resolve(managementHref(selectedNode))}>
							{$catalog.t('policy.openManagement')}
						</a>
						{#if selectedLinks.length}
							<h4>{$catalog.t('policy.directRelationships')}</h4>
							<ul class="policy-links">
								{#each selectedLinks as link, index (`${link.relationship}:${link.other.id}:${index}`)}
									<li>
										<span>{labels[link.relationship]}</span><button
											type="button"
											onclick={() => selectNode(link.other.id)}
											>{labels[link.other.type]} · {link.other.name}</button
										>
									</li>
								{/each}
							</ul>
						{/if}
					{:else}
						<h3 id="policy-inspector-title">{$catalog.t('policy.noSelection')}</h3>
						<p class="muted">{$catalog.t('policy.inspectorHelp')}</p>
					{/if}
				</aside>
			</div>
			<p class="policy-boundary">{$catalog.t('policy.accessBoundary')}</p>
		{/if}
	</section>
</ConsoleShell>
