import { describe, expect, it } from 'vitest';
import {
	decodePolicyMap,
	visiblePolicyIds,
	type PolicyMap
} from '../../../../src/lib/admin/policy-map';

const app = '10000000-0000-4000-8000-000000000001';
const role = '20000000-0000-4000-8000-000000000002';
const capability = '30000000-0000-4000-8000-000000000003';
const resource = '40000000-0000-4000-8000-000000000004';
const scope = '50000000-0000-4000-8000-000000000005';
const applicationNode = `application:${app}`;
const roleNode = `role:${role}`;
const capabilityNode = `capability:${capability}`;
const resourceNode = `resource:${resource}`;
const scopeNode = `scope:${scope}`;

function input(): unknown {
	return {
		application: { id: app, name: 'Example application', active: true },
		policy_revision: '42',
		complete: true,
		nodes: [
			{
				id: applicationNode,
				type: 'application',
				identifier: app,
				name: 'Example application',
				active: true
			},
			{ id: roleNode, type: 'role', identifier: role, name: 'Reviewer' },
			{
				id: capabilityNode,
				type: 'capability',
				identifier: capability,
				name: 'Read records',
				key: 'records:read',
				meaning: 'Read records owned by the current organization.',
				retired: false
			},
			{
				id: resourceNode,
				type: 'resource',
				identifier: resource,
				name: 'Records API',
				audience: 'https://records.example.test'
			},
			{
				id: scopeNode,
				type: 'scope',
				identifier: scope,
				name: 'records.read',
				resource_id: resourceNode
			}
		],
		edges: [
			edge(applicationNode, 'application_role', roleNode),
			edge(applicationNode, 'application_capability', capabilityNode),
			edge(applicationNode, 'application_resource', resourceNode),
			edge(roleNode, 'role_capability', capabilityNode),
			edge(resourceNode, 'resource_capability', capabilityNode),
			edge(resourceNode, 'resource_scope', scopeNode),
			edge(scopeNode, 'scope_capability', capabilityNode)
		]
	};
}
function edge(source: string, relationship: string, target: string) {
	return { id: `edge:${source}:${relationship}:${target}`, source, target, relationship };
}
function nodes(): unknown[] {
	return (input() as { nodes: unknown[] }).nodes;
}
function edges(): unknown[] {
	return (input() as { edges: unknown[] }).edges;
}
function change(path: string[], value: unknown) {
	type Mutable = Record<string, unknown> | unknown[];
	const data = structuredClone(input()) as Record<string, unknown>;
	let target: Mutable = data;
	for (const key of path.slice(0, -1)) {
		target = Array.isArray(target) ? (target[Number(key)] as Mutable) : (target[key] as Mutable);
	}
	const last = path.at(-1)!;
	if (Array.isArray(target)) target[Number(last)] = value;
	else target[last] = value;
	return data;
}
function valid(): PolicyMap {
	const result = decodePolicyMap(input());
	expect(result).not.toBeNull();
	return result!;
}

describe('decodePolicyMap', () => {
	it('keeps a complete, bounded, typed graph and its presentation fields', () => {
		const result = valid();
		expect(result.policy_revision).toBe('42');
		expect(result.nodes).toHaveLength(5);
		expect(result.edges).toHaveLength(7);
		expect(result.nodes.find((node) => node.type === 'capability')).toMatchObject({
			key: 'records:read',
			meaning: 'Read records owned by the current organization.',
			retired: false
		});
		expect(result.nodes.find((node) => node.type === 'scope')?.resource_id).toBe(resourceNode);
	});

	it('preserves hostile labels as inert plain-text values for escaped rendering', () => {
		const hostile = '<img src=x onerror=alert(1)>';
		const result = decodePolicyMap(change(['nodes', '1', 'name'], hostile));
		expect(result?.nodes.find((node) => node.type === 'role')?.name).toBe(hostile);
	});

	it.each([
		['top-level array', []],
		['missing application object', change(['application'], null)],
		['bad application id', change(['application', 'id'], 'x')],
		['empty application name', change(['application', 'name'], '')],
		['application activity missing', change(['application', 'active'], 'yes')],
		['malformed revision', change(['policy_revision'], '-1')],
		['revision overflow', change(['policy_revision'], '9223372036854775808')],
		['incomplete response', change(['complete'], false)],
		['missing node list', change(['nodes'], null)],
		['too many nodes', change(['nodes'], Array(2049).fill(null))],
		['missing edge list', change(['edges'], null)],
		['too many edges', change(['edges'], Array(8193).fill(null))],
		['null node', change(['nodes', '1'], null)],
		['unknown node kind', change(['nodes', '1', 'type'], 'client')],
		['invalid node identifier', change(['nodes', '1', 'identifier'], 'not-a-uuid')],
		[
			'nil node identifier',
			change(['nodes', '1', 'identifier'], '00000000-0000-0000-0000-000000000000')
		],
		['unqualified node id', change(['nodes', '1', 'id'], role)],
		['empty node name', change(['nodes', '1', 'name'], '')],
		['application node missing status', change(['nodes', '0', 'active'], null)],
		['application node differs from envelope', change(['nodes', '0', 'name'], 'Another app')],
		['duplicate node id', change(['nodes', '2', 'id'], roleNode)],
		['capability has no key', change(['nodes', '2', 'key'], '')],
		['capability has no meaning', change(['nodes', '2', 'meaning'], '')],
		['capability retirement is not boolean', change(['nodes', '2', 'retired'], 0)],
		['resource has no audience', change(['nodes', '3', 'audience'], '')],
		['scope has malformed resource id', change(['nodes', '4', 'resource_id'], 'role:bad')],
		['scope refers to non-resource node', change(['nodes', '4', 'resource_id'], roleNode)],
		[
			'scope refers to a missing resource',
			change(['nodes', '4', 'resource_id'], 'resource:60000000-0000-4000-8000-000000000006')
		],
		['application node absent', change(['nodes'], nodes().slice(1))],
		['duplicate application node', change(['nodes'], [...nodes(), nodes()[0]])],
		['edge is not an object', change(['edges', '0'], null)],
		['edge relationship unknown', change(['edges', '0', 'relationship'], 'user_role')],
		[
			'edge endpoint absent',
			change(['edges', '0', 'target'], 'role:60000000-0000-4000-8000-000000000006')
		],
		['edge uses incompatible node kinds', change(['edges', '0', 'source'], capabilityNode)],
		['edge identity disagrees with relation', change(['edges', '0', 'id'], 'wrong')],
		['duplicate edge', change(['edges'], [...edges(), edges()[0]])]
	])('rejects %s', (_name, value) => {
		expect(decodePolicyMap(value)).toBeNull();
	});
});

describe('visiblePolicyIds', () => {
	it('includes every item and relationship without filters', () => {
		const graph = valid();
		const visible = visiblePolicyIds(graph, {});
		expect(visible.nodeIds).toHaveLength(5);
		expect(visible.edgeIds).toHaveLength(7);
	});

	it('filters to a relation and its incident nodes', () => {
		const visible = visiblePolicyIds(valid(), { relationship: 'resource_scope' });
		expect(visible.nodeIds).toEqual(new Set([resourceNode, scopeNode]));
		expect(visible.edgeIds).toEqual(new Set([`edge:${resourceNode}:resource_scope:${scopeNode}`]));
	});

	it('filters by item type and keeps its direct explanatory context', () => {
		const graph = valid();
		const selected = visiblePolicyIds(graph, { entity: 'scope' });
		expect(selected.nodeIds).toEqual(new Set([resourceNode, scopeNode, capabilityNode]));
		expect(selected.edgeIds).toHaveLength(3);
	});

	it('searches identifiers, descriptions and names, keeping one-hop neighbors', () => {
		const graph = valid();
		expect(visiblePolicyIds(graph, { search: 'OWNED BY' }).nodeIds).toEqual(
			new Set([applicationNode, roleNode, capabilityNode, resourceNode, scopeNode])
		);
		const selected = visiblePolicyIds(graph, { search: role });
		expect(selected.nodeIds).toEqual(new Set([applicationNode, roleNode, capabilityNode]));
		expect(selected.edgeIds).toHaveLength(3);
	});

	it('focuses one item and its direct neighbors without expanding transitively', () => {
		const graph = valid();
		const selected = visiblePolicyIds(graph, { focusId: resourceNode });
		expect(selected.nodeIds).toEqual(
			new Set([applicationNode, resourceNode, capabilityNode, scopeNode])
		);
		expect(selected.edgeIds).toHaveLength(5);
		expect(visiblePolicyIds(graph, { focusId: 'missing' }).nodeIds).toEqual(new Set());
	});

	it('returns no rows for a search with no matches', () => {
		const selected = visiblePolicyIds(valid(), { search: 'not-present' });
		expect(selected.nodeIds.size).toBe(0);
		expect(selected.edgeIds.size).toBe(0);
	});

	it('keeps matching isolated items visible when they have no relationships', () => {
		const graph = valid();
		const emptyRole = {
			id: 'role:60000000-0000-4000-8000-000000000006',
			type: 'role' as const,
			identifier: '60000000-0000-4000-8000-000000000006',
			name: 'Empty reviewer'
		};
		graph.nodes.push(emptyRole);
		const selected = visiblePolicyIds(graph, { search: 'Empty reviewer' });
		expect(selected.nodeIds).toEqual(new Set([emptyRole.id]));
		expect(selected.edgeIds.size).toBe(0);
	});

	it('drops dangling neighbors if called with an untrusted graph object', () => {
		const graph = valid();
		graph.edges.push({
			id: 'untrusted dangling relationship',
			source: 'unknown:60000000-0000-4000-8000-000000000006',
			target: roleNode,
			relationship: 'role_capability'
		});
		const selected = visiblePolicyIds(graph, { entity: 'role' });
		expect(selected.nodeIds).not.toContain('unknown:60000000-0000-4000-8000-000000000006');
		expect(selected.edgeIds).not.toContain('untrusted dangling relationship');
	});
});
