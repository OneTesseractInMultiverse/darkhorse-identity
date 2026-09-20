import { expect, it, vi } from 'vitest';
import { catalogApi, parameters, detailPath } from '../../../../src/lib/admin/catalog';
import {
	item,
	page,
	view,
	registered,
	reference,
	counter
} from '../../../../src/lib/admin/catalog-decode';
import { binding } from '../../../../src/lib/admin/catalog-changes';
const id = '00000000-0000-0000-0000-000000000001';
const role = { kind: 'role' as const, id, name: 'Reader' };
const app = {
	kind: 'application' as const,
	id,
	name: 'Portal',
	active: true,
	revision: '9007199254740993',
	owner_id: id,
	owner_email: 'owner@example.com'
};
const client = {
	kind: 'client' as const,
	id,
	name: 'Web',
	application_id: id,
	active: true,
	revision: '2',
	redirect_uris: ['https://web.example/cb'],
	resource_ids: [],
	scope_ids: [],
	refresh_tokens: false,
	token_endpoint_auth_method: 'client_secret_basic',
	secrets: [{ id, created_ms: 1, expires_ms: null }]
};
it('keeps large revisions exact and rejects malformed references and unsafe counters', () => {
	for (const v of ['0', '9007199254740993', '9223372036854775807']) expect(counter(v)).toBe(true);
	for (const v of [0, '01', '-1', '9223372036854775808', '1e2', null])
		expect(counter(v)).toBe(false);
	expect(reference(id)).toBe(true);
	for (const v of [
		'',
		id.toUpperCase().replace('1', 'A'),
		'00000000-0000-0000-0000-000000000000',
		null
	])
		expect(reference(v)).toBe(false);
	expect(item(app)?.revision).toBe('9007199254740993');
	expect(item({ ...app, revision: 9007199254740992 })).toBeNull();
});
it('decodes bounded catalog records and strips any unrequested raw secret', () => {
	expect(item({ ...client, client_secret: 'sensitive' })).not.toHaveProperty('client_secret');
	const cap = { kind: 'capability', id, name: 'users.read', meaning: 'Read users', active: true };
	const resource = { kind: 'resource', id, name: 'API', application_id: id, audience: 'urn:api' };
	const scope = { kind: 'scope', id, name: 'read', application_id: id, resource_id: id };
	for (const record of [app, client, cap, resource, scope, role])
		expect(item(record)).not.toBeNull();
	for (const record of [
		null,
		[],
		{},
		{ ...client, secrets: [{ id, created_ms: -1, expires_ms: null }] },
		{ ...client, token_endpoint_auth_method: 'none' },
		{ ...client, resource_ids: [id, id] },
		{ ...cap, meaning: '' },
		{ ...resource, application_id: 'bad' },
		{ ...scope, resource_id: 'bad' },
		{ ...role, kind: 'unknown' }
	])
		expect(item(record)).toBeNull();
	expect(page({ items: [role], next: null, policy_revision: '7' })?.items).toEqual([role]);
	for (const value of [
		{ items: [role, role], next: null, policy_revision: '7' },
		{ items: [], next: 'bad', policy_revision: '7' },
		{ items: Array(101).fill(role), next: null, policy_revision: '7' }
	])
		expect(page(value)).toBeNull();
	expect(
		view({ item: role, applications: [app], capabilities: [cap], policy_revision: '7' })
	).not.toBeNull();
	expect(
		view({ item: role, applications: [role], capabilities: [], policy_revision: '7' })
	).toBeNull();
	expect(registered({ record: client, client_secret: 'a'.repeat(64) })?.client_secret).toHaveLength(
		64
	);
	expect(registered({ record: role, client_secret: 'a'.repeat(64) })).toBeNull();
	expect(registered({ record: client, client_secret: 'invalid' })).toBeNull();
});
it('uses same-origin no-store requests and never retries uncertain writes', async () => {
	const fetcher = vi
		.fn()
		.mockResolvedValueOnce(
			new Response(JSON.stringify({ items: [app], next: null, policy_revision: '7' }), {
				status: 200
			})
		)
		.mockRejectedValueOnce(new Error('lost response'));
	const api = catalogApi(fetcher);
	expect((await api.list('applications', { search: ' Portal ', after: id })).kind).toBe('ready');
	expect(fetcher.mock.calls[0][1]).toMatchObject({ cache: 'no-store', credentials: 'same-origin' });
	expect(await api.register({ operation: 'rotate_secret', revision: '9007199254740993' })).toEqual({
		kind: 'uncertain'
	});
	expect(fetcher).toHaveBeenCalledTimes(2);
	expect(fetcher.mock.calls[1][1]).toMatchObject({
		headers: { 'x-darkhorse-csrf': '1' },
		body: '{"operation":"rotate_secret","revision":"9007199254740993"}'
	});
	expect(parameters({ search: ' % ', status: '' })).toContain('search=%25');
});
it('maps rejected and malformed responses without disclosing server details', async () => {
	for (const [status, body, kind] of [
		[401, {}, 'signed-out'],
		[403, {}, 'forbidden'],
		[403, { error: 'recent_authentication_required' }, 'recent'],
		[409, {}, 'conflict'],
		[400, {}, 'invalid'],
		[500, { error: 'database details' }, 'uncertain'],
		[200, {}, 'uncertain']
	] as const) {
		const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(body), { status }));
		expect(await catalogApi(fetcher).register({ operation: 'x' })).toEqual({ kind });
	}
	for (const status of [401, 403, 500]) {
		const fetcher = vi.fn().mockResolvedValue(new Response('{}', { status }));
		expect((await catalogApi(fetcher).detail(role)).kind).toBe(
			status === 401 ? 'signed-out' : status === 403 ? 'forbidden' : 'unavailable'
		);
	}
	const fetcher = vi
		.fn()
		.mockResolvedValueOnce(new Response(JSON.stringify({ record: app }), { status: 200 }))
		.mockResolvedValueOnce(
			new Response(JSON.stringify({ target: { id, kind: 'roles' }, policy_revision: '9' }), {
				status: 200
			})
		);
	const api = catalogApi(fetcher);
	expect((await api.register({ operation: 'create_application' })).kind).toBe('saved');
	expect(await api.change('8', { operation: 'create_role' })).toEqual({
		kind: 'saved',
		data: { policy_revision: '9' }
	});
});
it('targets typed catalog bindings without wildcard applications', () => {
	expect(detailPath(app)).toBe(`/api/admin/console/applications/${id}`);
	expect(detailPath(client)).toContain(`/clients/${id}`);
	expect(detailPath(role)).toBe(`/api/admin/catalog/roles/${id}`);
	expect(binding(role, id, true, true)).toEqual({
		operation: 'role_binding',
		application_id: id,
		role_id: id,
		bound: true
	});
	expect(binding(role, id, false, false)).toEqual({
		operation: 'role_capability',
		role_id: id,
		capability_id: id,
		granted: false
	});
	const capability = { kind: 'capability' as const, id, name: 'read' };
	expect(binding(capability, id, true, false).operation).toBe('capability_binding');
	const resource = { kind: 'resource' as const, id, name: 'API', application_id: id };
	expect(binding(resource, id, false, true)).toMatchObject({
		operation: 'resource_capability',
		exposed: true,
		application_id: id
	});
	const scope = { ...resource, kind: 'scope' as const, resource_id: id };
	expect(detailPath(scope)).toContain('resource_id=');
	expect(binding(scope, id, false, false)).toMatchObject({
		operation: 'scope_capability',
		included: false
	});
});
it('fails closed on unavailable reads and invalid client metadata', async () => {
	expect(item({ ...client, application_id: 'invalid' })).toBeNull();
	expect(
		(await catalogApi(vi.fn().mockRejectedValue(new Error('unavailable'))).list('roles')).kind
	).toBe('unavailable');
	expect(
		(await catalogApi(vi.fn().mockResolvedValue(new Response('{}', { status: 200 }))).list('roles'))
			.kind
	).toBe('unavailable');
});
