<script lang="ts">
	import { SvelteFlow, Background, BackgroundVariant, Controls, MiniMap } from '@xyflow/svelte';
	import '@xyflow/svelte/dist/style.css';
	import { policyFlowElements } from '$lib/admin/policy-flow';
	import type { PolicyLabels, PolicyMap } from '$lib/admin/policy-map';
	let {
		graph,
		labels,
		visibleNodes,
		visibleEdges,
		selectedId,
		flowLabels,
		onSelect
	}: {
		graph: PolicyMap;
		labels: PolicyLabels;
		visibleNodes: Set<string>;
		visibleEdges: Set<string>;
		selectedId: string | null;
		flowLabels: {
			region: string;
			controls: string;
			zoomIn: string;
			zoomOut: string;
			fit: string;
			toggle: string;
			minimap: string;
			node: string;
			edge: string;
		};
		onSelect: (id: string | null) => void;
	} = $props();
	const elements = $derived(policyFlowElements(graph, labels));
	const nodes = $derived(
		elements.nodes
			.filter((node) => visibleNodes.has(node.id))
			.map((node) => ({ ...node, selected: node.id === selectedId }))
	);
	const edges = $derived(elements.edges.filter((edge) => visibleEdges.has(edge.id)));
</script>

<div class="policy-flow" role="region" aria-label={flowLabels.region}>
	<SvelteFlow
		{nodes}
		{edges}
		fitView
		fitViewOptions={{ padding: 0.18, maxZoom: 1 }}
		nodesDraggable={false}
		nodesConnectable={false}
		elementsSelectable
		nodesFocusable
		edgesFocusable
		deleteKey={null}
		panOnDrag
		zoomOnScroll
		onlyRenderVisibleElements
		colorMode="dark"
		ariaLabelConfig={{
			'controls.ariaLabel': flowLabels.controls,
			'controls.zoomIn.ariaLabel': flowLabels.zoomIn,
			'controls.zoomOut.ariaLabel': flowLabels.zoomOut,
			'controls.fitView.ariaLabel': flowLabels.fit,
			'controls.interactive.ariaLabel': flowLabels.toggle,
			'minimap.ariaLabel': flowLabels.minimap,
			'node.a11yDescription.default': flowLabels.node,
			'edge.a11yDescription.default': flowLabels.edge
		}}
		onnodeclick={({ node }) => onSelect(node.id)}
		onselectionchange={({ nodes }) => onSelect(nodes[0]?.id ?? null)}
	>
		<Background variant={BackgroundVariant.Dots} gap={24} size={1} patternColor="#355240" />
		<Controls aria-label={flowLabels.controls} />
		<MiniMap ariaLabel={flowLabels.minimap} nodeColor="#91e4b4" bgColor="#101813" />
	</SvelteFlow>
</div>

<style>
	.policy-flow {
		width: 100%;
		height: min(70vh, 760px);
		min-height: 480px;
		border: 1px solid var(--console-line);
		border-radius: 16px;
		overflow: hidden;
		background: #0c130f;
	}
</style>
