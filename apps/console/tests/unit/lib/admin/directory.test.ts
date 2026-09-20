import { expect, it, vi } from 'vitest';
import { directoryApi } from '../../../../src/lib/admin/directory';
const id = '00000000-0000-0000-0000-000000000001';
const record = {
	id,
	email: 'ada@example.com',
	first_name: 'Ada',
	last_name: 'Lovelace',
	active: true,
	administrator: true,
	email_verified: false,
	revision: '9223372036854775807'
};
const response = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
it('keeps queries literal, revision strings exact and request credentials same-origin', async () => {
	const fetcher = vi
		.fn()
		.mockResolvedValueOnce(response({ actor: id, items: [record], next: null }))
		.mockResolvedValue(response(record));
	const api = directoryApi(fetcher);
	expect((await api.list({ search: 'a&status=inactive', status: 'active' })).kind).toBe('ready');
	expect(fetcher.mock.calls[0][0]).toContain('search=a%26status%3Dinactive');
	expect(await api.update(record, { kind: 'status', active: false })).toEqual({
		kind: 'saved',
		user: record
	});
	expect(fetcher.mock.calls[1][1]).toMatchObject({
		credentials: 'same-origin',
		cache: 'no-store',
		headers: { 'x-darkhorse-csrf': '1' }
	});
	expect(JSON.parse(fetcher.mock.calls[1][1].body).revision).toBe(record.revision);
});
it('rejects malformed projections and classifies denied and uncertain outcomes without retrying', async () => {
	for (const body of [null, {}, { ...record, revision: 2 }, { ...record, id: 'bad' }]) {
		const fetcher = vi.fn().mockResolvedValue(response(body));
		expect(await directoryApi(fetcher).user(id)).toEqual({ kind: 'unavailable' });
	}
	for (const [status, error, kind] of [
		[401, '', 'signed-out'],
		[403, 'administrator_required', 'forbidden'],
		[403, 'recent_authentication_required', 'recent'],
		[409, 'revision_conflict', 'conflict'],
		[409, 'last_administrator', 'last-administrator'],
		[400, '', 'invalid'],
		[503, '', 'uncertain']
	] as const) {
		const fetcher = vi.fn().mockResolvedValue(response({ error }, status));
		expect(await directoryApi(fetcher).update(record, { kind: 'status', active: false })).toEqual({
			kind
		});
		expect(fetcher).toHaveBeenCalledTimes(1);
	}
	const offline = directoryApi(vi.fn().mockRejectedValue(new Error('offline')));
	expect(await offline.user(id)).toEqual({ kind: 'unavailable' });
	expect(await offline.update(record, { kind: 'status', active: false })).toEqual({
		kind: 'uncertain'
	});
});
it('bounds pages and application-role projections and rejects cross-application selection', async () => {
	const valid = {
		user: record,
		policy_revision: '1',
		applications: [{ id, name: 'Calendar', active: true }],
		selected: id,
		roles: [{ id, name: 'Reader', assigned: false }]
	};
	const api = directoryApi(vi.fn().mockResolvedValue(response(valid)));
	expect((await api.access(id, id)).kind).toBe('ready');
	for (const body of [
		{ ...valid, policy_revision: '01' },
		{ ...valid, roles: Array(129).fill(valid.roles[0]) },
		{ ...valid, applications: [] },
		{ ...valid, selected: null }
	]) {
		expect(await directoryApi(vi.fn().mockResolvedValue(response(body))).access(id)).toEqual({
			kind: 'unavailable'
		});
	}
	for (const body of [
		{ actor: id, items: [record, record], next: null },
		{ actor: id, items: Array(101).fill(record), next: null },
		{ actor: id, items: [], next: 'bad' }
	]) {
		expect(
			await directoryApi(vi.fn().mockResolvedValue(response(body))).list({ search: '', status: '' })
		).toEqual({ kind: 'unavailable' });
	}
});
it('treats denied reads and invalid role catalog records as unavailable authority', async () => {
	for (const [status, kind] of [
		[401, 'signed-out'],
		[403, 'forbidden'],
		[503, 'unavailable']
	] as const)
		expect(await directoryApi(vi.fn().mockResolvedValue(response({}, status))).user(id)).toEqual({
			kind
		});
	const base = {
		user: record,
		policy_revision: '1',
		applications: [{ id, name: 'Portal', active: true }],
		selected: id,
		roles: [{ id, name: 'Reader', assigned: false }]
	};
	for (const body of [
		{ ...base, applications: [{ id, name: 'Portal', active: 'yes' }] },
		{ ...base, roles: [{ id, name: 'Reader', assigned: 'yes' }] }
	])
		expect(await directoryApi(vi.fn().mockResolvedValue(response(body))).access(id)).toEqual({
			kind: 'unavailable'
		});
});
