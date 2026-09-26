import type { Edge, Node } from '@xyflow/svelte';

export type PolicyNodeType = 'application' | 'role' | 'capability' | 'resource' | 'scope';
export type Relationship =
	| 'application_role'
	| 'application_capability'
	| 'application_resource'
	| 'role_capability'
	| 'resource_capability'
	| 'resource_scope'
	| 'scope_capability';
export type PolicyNode = {
	id: string;
	type: PolicyNodeType;
	identifier: string;
	name: string;
	active?: boolean;
	key?: string;
	meaning?: string;
	retired?: boolean;
	audience?: string;
	resource_id?: string;
};
export type PolicyEdge = { id: string; source: string; target: string; relationship: Relationship };
export type PolicyMap = {
	application: { id: string; name: string; active: boolean };
	policy_revision: string;
	complete: true;
	nodes: PolicyNode[];
	edges: PolicyEdge[];
};
export type PolicyLabels = Record<PolicyNodeType | Relationship, string>;
export type PolicyFlowNode = Node<{ label: string }, 'default'>;
export type PolicyFlowEdge = Edge<Record<string, never>, 'default'>;

const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const relationships: Relationship[] = [
	'application_role',
	'application_capability',
	'application_resource',
	'role_capability',
	'resource_capability',
	'resource_scope',
	'scope_capability'
];
const types: PolicyNodeType[] = ['application', 'role', 'capability', 'resource', 'scope'];
const compatible: Record<Relationship, [PolicyNodeType, PolicyNodeType]> = {
	application_role: ['application', 'role'],
	application_capability: ['application', 'capability'],
	application_resource: ['application', 'resource'],
	role_capability: ['role', 'capability'],
	resource_capability: ['resource', 'capability'],
	resource_scope: ['resource', 'scope'],
	scope_capability: ['scope', 'capability']
};
const uuidText = (value: unknown): value is string =>
	typeof value === 'string' && uuid.test(value) && value !== '00000000-0000-0000-0000-000000000000';
const labelText = (value: unknown, max = 2048): value is string =>
	typeof value === 'string' && value.length > 0 && Array.from(value).length <= max;
const object = (value: unknown): value is Record<string, unknown> =>
	typeof value === 'object' && value !== null && !Array.isArray(value);

/** Strictly decode the bounded, complete graph returned by the protected admin endpoint. */
export function decodePolicyMap(value: unknown): PolicyMap | null {
	if (!object(value) || !object(value.application)) return null;
	const app = value.application;
	if (
		!uuidText(app.id) ||
		!labelText(app.name) ||
		typeof app.active !== 'boolean' ||
		typeof value.policy_revision !== 'string' ||
		!/^(0|[1-9][0-9]{0,18})$/.test(value.policy_revision) ||
		BigInt(value.policy_revision) > 9223372036854775807n ||
		value.complete !== true ||
		!Array.isArray(value.nodes) ||
		value.nodes.length < 1 ||
		value.nodes.length > 2048 ||
		!Array.isArray(value.edges) ||
		value.edges.length > 8192
	)
		return null;

	const nodes: PolicyNode[] = [];
	const byId = new Map<string, PolicyNode>();
	for (const entry of value.nodes) {
		if (
			!object(entry) ||
			typeof entry.type !== 'string' ||
			!types.includes(entry.type as PolicyNodeType) ||
			!uuidText(entry.identifier) ||
			entry.id !== `${entry.type}:${entry.identifier}` ||
			!labelText(entry.name)
		)
			return null;
		const type = entry.type as PolicyNodeType;
		let node: PolicyNode;
		if (type === 'application') {
			if (
				typeof entry.active !== 'boolean' ||
				entry.identifier !== app.id ||
				entry.name !== app.name
			)
				return null;
			node = {
				id: entry.id as string,
				type,
				identifier: entry.identifier,
				name: entry.name,
				active: entry.active
			};
		} else if (type === 'role') {
			node = { id: entry.id as string, type, identifier: entry.identifier, name: entry.name };
		} else if (type === 'capability') {
			if (
				!labelText(entry.key, 200) ||
				!labelText(entry.meaning, 1000) ||
				typeof entry.retired !== 'boolean'
			)
				return null;
			node = {
				id: entry.id as string,
				type,
				identifier: entry.identifier,
				name: entry.name,
				key: entry.key,
				meaning: entry.meaning,
				retired: entry.retired
			};
		} else if (type === 'resource') {
			if (!labelText(entry.audience)) return null;
			node = {
				id: entry.id as string,
				type,
				identifier: entry.identifier,
				name: entry.name,
				audience: entry.audience
			};
		} else {
			if (
				typeof entry.resource_id !== 'string' ||
				!entry.resource_id.startsWith('resource:') ||
				!uuidText(entry.resource_id.slice('resource:'.length))
			)
				return null;
			node = {
				id: entry.id as string,
				type,
				identifier: entry.identifier,
				name: entry.name,
				resource_id: entry.resource_id
			};
		}
		if (byId.has(node.id)) return null;
		byId.set(node.id, node);
		nodes.push(node);
	}
	if (
		byId.get(`application:${app.id}`)?.type !== 'application' ||
		[...byId.values()].filter((node) => node.type === 'application').length !== 1
	)
		return null;
	for (const node of nodes) {
		if (node.type === 'scope') {
			const resource = byId.get(node.resource_id!);
			if (!resource || resource.type !== 'resource') return null;
		}
	}

	const edges: PolicyEdge[] = [];
	const edgeIds = new Set<string>();
	for (const entry of value.edges) {
		if (
			!object(entry) ||
			typeof entry.source !== 'string' ||
			typeof entry.target !== 'string' ||
			typeof entry.relationship !== 'string' ||
			!relationships.includes(entry.relationship as Relationship)
		)
			return null;
		const relationship = entry.relationship as Relationship;
		const source = byId.get(entry.source);
		const target = byId.get(entry.target);
		if (
			!source ||
			!target ||
			source.type !== compatible[relationship][0] ||
			target.type !== compatible[relationship][1]
		)
			return null;
		const id = `edge:${entry.source}:${relationship}:${entry.target}`;
		if (entry.id !== id || edgeIds.has(id)) return null;
		edgeIds.add(id);
		edges.push({ id, source: entry.source, target: entry.target, relationship });
	}
	return {
		application: { id: app.id, name: app.name, active: app.active },
		policy_revision: value.policy_revision,
		complete: true,
		nodes,
		edges
	};
}

/** Return all matching nodes and their direct policy neighbors for a focused search. */
export function visiblePolicyIds(
	graph: PolicyMap,
	options: {
		search?: string;
		relationship?: Relationship | '';
		entity?: PolicyNodeType | '';
		focusId?: string | null;
	}
): { nodeIds: Set<string>; edgeIds: Set<string> } {
	const relationEdges = graph.edges.filter(
		(edge) => !options.relationship || edge.relationship === options.relationship
	);
	const query = options.search?.trim().toLocaleLowerCase();
	const anchors = new Set(
		graph.nodes
			.filter(
				(node) =>
					(!options.entity || node.type === options.entity) &&
					(!options.focusId || node.id === options.focusId) &&
					(!query ||
						[node.name, node.identifier, node.key, node.meaning, node.audience].some((value) =>
							value?.toLocaleLowerCase().includes(query)
						))
			)
			.map((node) => node.id)
	);
	const filtering = Boolean(options.relationship || options.entity || query || options.focusId);
	let included = new Set(anchors);
	if (filtering && !options.entity && !query && !options.focusId && options.relationship) {
		included = new Set(relationEdges.flatMap((edge) => [edge.source, edge.target]));
	} else if (!filtering) included = new Set(graph.nodes.map((node) => node.id));
	else {
		for (const edge of relationEdges) {
			if (anchors.has(edge.source)) included.add(edge.target);
			if (anchors.has(edge.target)) included.add(edge.source);
		}
	}
	const knownNodeIds = new Set(graph.nodes.map((node) => node.id));
	for (const nodeId of included) if (!knownNodeIds.has(nodeId)) included.delete(nodeId);
	const edges = relationEdges.filter(
		(edge) => included.has(edge.source) && included.has(edge.target)
	);
	return {
		nodeIds: new Set(included),
		edgeIds: new Set(edges.map((edge) => edge.id))
	};
}
