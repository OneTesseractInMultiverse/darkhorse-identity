import { expect, it } from 'vitest';
import { MarkerType } from '@xyflow/svelte';
import {
	canRenderPolicyFlow,
	MAX_POLICY_FLOW_EDGES,
	MAX_POLICY_FLOW_NODES,
	policyFlowElements
} from '../../../../src/lib/admin/policy-flow';
import type { PolicyLabels, PolicyMap } from '../../../../src/lib/admin/policy-map';

const graph: PolicyMap = {
	application: { id: '10000000-0000-4000-8000-000000000001', name: 'Console', active: true },
	policy_revision: '8',
	complete: true,
	nodes: [
		{
			id: 'application:10000000-0000-4000-8000-000000000001',
			type: 'application',
			identifier: '10000000-0000-4000-8000-000000000001',
			name: 'Console',
			active: true
		},
		{
			id: 'role:20000000-0000-4000-8000-000000000002',
			type: 'role',
			identifier: '20000000-0000-4000-8000-000000000002',
			name: 'Auditor'
		},
		{
			id: 'capability:30000000-0000-4000-8000-000000000003',
			type: 'capability',
			identifier: '30000000-0000-4000-8000-000000000003',
			name: 'audit.read',
			key: 'audit.read',
			meaning: 'Read audit records',
			retired: false
		},
		{
			id: 'resource:40000000-0000-4000-8000-000000000004',
			type: 'resource',
			identifier: '40000000-0000-4000-8000-000000000004',
			name: 'Audit API',
			audience: 'https://audit.example.test'
		},
		{
			id: 'scope:50000000-0000-4000-8000-000000000005',
			type: 'scope',
			identifier: '50000000-0000-4000-8000-000000000005',
			name: 'audit.read',
			resource_id: 'resource:40000000-0000-4000-8000-000000000004'
		}
	],
	edges: [
		{
			id: 'edge:application:10000000-0000-4000-8000-000000000001:application_role:role:20000000-0000-4000-8000-000000000002',
			source: 'application:10000000-0000-4000-8000-000000000001',
			target: 'role:20000000-0000-4000-8000-000000000002',
			relationship: 'application_role'
		},
		{
			id: 'edge:role:20000000-0000-4000-8000-000000000002:role_capability:capability:30000000-0000-4000-8000-000000000003',
			source: 'role:20000000-0000-4000-8000-000000000002',
			target: 'capability:30000000-0000-4000-8000-000000000003',
			relationship: 'role_capability'
		}
	]
};
const labels: PolicyLabels = {
	application: 'Application',
	role: 'Role',
	capability: 'Capability',
	resource: 'Resource',
	scope: 'Scope',
	application_role: 'Application includes role',
	application_capability: 'Application binds capability',
	application_resource: 'Application protects resource',
	role_capability: 'Role grants capability',
	resource_capability: 'Resource grants capability',
	resource_scope: 'Resource contains scope',
	scope_capability: 'Scope grants capability'
};

it('keeps interactive rendering within explicit node and edge bounds', () => {
	expect(canRenderPolicyFlow(1, 0)).toBe(true);
	expect(canRenderPolicyFlow(MAX_POLICY_FLOW_NODES, MAX_POLICY_FLOW_EDGES)).toBe(true);
	expect(canRenderPolicyFlow(MAX_POLICY_FLOW_NODES + 1, 0)).toBe(false);
	expect(canRenderPolicyFlow(1, MAX_POLICY_FLOW_EDGES + 1)).toBe(false);
	expect(canRenderPolicyFlow(0, 0)).toBe(false);
	expect(canRenderPolicyFlow(-1, 0)).toBe(false);
	expect(canRenderPolicyFlow(1.5, 0)).toBe(false);
	expect(canRenderPolicyFlow(Number.NaN, 0)).toBe(false);
});

it('creates a stable, readable, non-editable layered graph with accessible labels', () => {
	const { nodes, edges } = policyFlowElements(graph, labels);
	expect(nodes).toHaveLength(5);
	expect(edges).toHaveLength(2);
	expect(nodes.map(({ id, position }) => [id.split(':')[0], position.x])).toEqual([
		['application', 0],
		['role', 320],
		['capability', 960],
		['resource', 320],
		['scope', 640]
	]);
	expect(nodes[1]).toMatchObject({
		data: { label: '◈ Role · Auditor' },
		ariaLabel: 'Role: Auditor',
		focusable: true,
		draggable: false,
		connectable: false,
		deletable: false
	});
	expect(edges[1]).toMatchObject({
		label: 'Role grants capability',
		ariaLabel: 'Role grants capability',
		markerEnd: { type: MarkerType.ArrowClosed },
		deletable: false,
		selectable: true
	});
});

it('keeps hostile display data as text rather than interpreting markup', () => {
	const hostile = '<img src=x onerror=alert(1)>';
	const value = {
		...graph,
		nodes: graph.nodes.map((node) => (node.type === 'role' ? { ...node, name: hostile } : node))
	};
	const { nodes } = policyFlowElements(value, labels);
	expect(nodes[1].data.label).toBe(`◈ Role · ${hostile}`);
});
