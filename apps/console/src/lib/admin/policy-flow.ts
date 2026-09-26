import { MarkerType, type Edge, type Node } from '@xyflow/svelte';
import type { PolicyLabels, PolicyMap, PolicyNodeType } from './policy-map';

export type PolicyFlowNode = Node<{ label: string }, 'default'>;
export type PolicyFlowEdge = Edge<Record<string, never>, 'default'>;
export const MAX_POLICY_FLOW_NODES = 512;
export const MAX_POLICY_FLOW_EDGES = 2048;

/** Keep interactive graph work bounded; full policy data remains available in the table. */
export function canRenderPolicyFlow(nodeCount: number, edgeCount: number): boolean {
	return (
		Number.isInteger(nodeCount) &&
		Number.isInteger(edgeCount) &&
		nodeCount > 0 &&
		nodeCount <= MAX_POLICY_FLOW_NODES &&
		edgeCount >= 0 &&
		edgeCount <= MAX_POLICY_FLOW_EDGES
	);
}

const glyph: Record<PolicyNodeType, string> = {
	application: '◉',
	role: '◈',
	capability: '◆',
	resource: '▤',
	scope: '◌'
};

/** Layer nodes in a stable four-column layout so filters never rearrange the graph. */
export function policyFlowElements(
	graph: PolicyMap,
	labels: PolicyLabels
): {
	nodes: PolicyFlowNode[];
	edges: PolicyFlowEdge[];
} {
	const columns: PolicyNodeType[][] = [
		['application'],
		['role', 'resource'],
		['scope'],
		['capability']
	];
	const rowCounts = columns.map(
		(typesInColumn) => graph.nodes.filter((node) => typesInColumn.includes(node.type)).length
	);
	const maxRows = Math.max(...rowCounts);
	const positions = new Map<string, { x: number; y: number }>();
	columns.forEach((typesInColumn, column) => {
		const rows = graph.nodes.filter((node) => typesInColumn.includes(node.type));
		const verticalOffset = ((maxRows - rows.length) * 144) / 2;
		rows.forEach((node, row) =>
			positions.set(node.id, { x: column * 320, y: verticalOffset + row * 144 })
		);
	});
	const nodes: PolicyFlowNode[] = graph.nodes.map((node) => ({
		id: node.id,
		type: 'default',
		position: positions.get(node.id)!,
		data: { label: `${glyph[node.type]} ${labels[node.type]} · ${node.name}` },
		ariaLabel: `${labels[node.type]}: ${node.name}`,
		focusable: true,
		draggable: false,
		connectable: false,
		deletable: false,
		class: `policy-flow-node policy-flow-node-${node.type}`
	}));
	const edges: PolicyFlowEdge[] = graph.edges.map((edge) => ({
		id: edge.id,
		type: 'default',
		source: edge.source,
		target: edge.target,
		label: labels[edge.relationship],
		ariaLabel: labels[edge.relationship],
		markerEnd: { type: MarkerType.ArrowClosed, color: '#91e4b4' },
		focusable: true,
		deletable: false,
		selectable: true,
		style: 'stroke: #91e4b4; stroke-width: 1.5px',
		labelStyle: 'fill: #c4d8cb; font-size: 11px'
	}));
	return { nodes, edges };
}
