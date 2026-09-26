import { describe, expect, it, vi } from 'vitest';
import { keyApi, type Creation } from '../../../src/lib/personal-keys';
import { options } from './key-fixtures';
const id = (n: number) => `00000000-0000-0000-0000-${n.toString(16).padStart(12, '0')}`;
const record = {
	id: id(1),
	name: 'Worker',
	application_id: id(2),
	created_ms: 1000,
	expires_ms: null,
	active: true,
	grants: [{ resource_id: id(3), capabilities: [id(4)] }]
};
const creation: Creation = {
	name: 'Worker',
	application_id: id(2),
	policy_revision: '9',
	expiration: { kind: 'never' },
	grants: [{ resource_id: id(3), selection: { kind: 'all' } }]
};
describe('personal key transport', () => {
	it('loads bounded issuance options and produces typed failures without server text', async () => {
		const fetcher = vi
			.fn()
			.mockResolvedValueOnce(new Response(JSON.stringify(options)))
			.mockResolvedValueOnce(new Response('{}'))
			.mockResolvedValueOnce(new Response('{}', { status: 401 }));
		expect(await keyApi(fetcher).options(id(3))).toEqual({ kind: 'ready', value: options });
		expect(fetcher.mock.calls[0][0]).toBe(`/api/security/keys/options?after=${id(3)}`);
		expect(await keyApi(fetcher).options()).toEqual({ kind: 'unavailable' });
		expect(await keyApi(fetcher).list()).toEqual({ kind: 'signed-out' });
		for (const body of ['not json', '{}', JSON.stringify({ error: 'untrusted message' })]) {
			const failed = vi.fn().mockResolvedValue(new Response(body, { status: 403 }));
			expect(await keyApi(failed).revoke(id(1))).toEqual({ kind: 'denied' });
		}
		const broken = vi.fn().mockRejectedValue(new Error('untrusted'));
		expect(await keyApi(broken).revoke(id(1))).toEqual({ kind: 'uncertain' });
	});
	it('uses protected same-origin requests and exposes secrets only from a committed creation', async () => {
		const fetcher = vi
			.fn()
			.mockResolvedValueOnce(
				new Response(JSON.stringify({ items: [{ ...record, secret: 'unexpected' }], next: null }))
			)
			.mockResolvedValueOnce(
				new Response(JSON.stringify({ key: record, secret: `dk_${'ab'.repeat(32)}` }), {
					status: 201
				})
			)
			.mockResolvedValueOnce(new Response(null, { status: 204 }));
		const api = keyApi(fetcher);
		const page = await api.list(id(1));
		expect(page.kind).toBe('ready');
		expect(JSON.stringify(page)).not.toContain('unexpected');
		expect((await api.create(creation)).kind).toBe('created');
		expect((await api.revoke(id(1))).kind).toBe('revoked');
		expect(fetcher.mock.calls[0][0]).toBe(`/api/security/keys?after=${id(1)}`);
		expect(fetcher.mock.calls[1][1]).toMatchObject({
			credentials: 'same-origin',
			cache: 'no-store',
			headers: { 'x-darkhorse-csrf': '1' },
			body: JSON.stringify(creation)
		});
	});
	it('never retries lost or rejected writes and rejects malformed success responses', async () => {
		for (const status of [400, 401, 403, 404, 409, 429, 500]) {
			const fetcher = vi.fn().mockResolvedValue(
				new Response(
					JSON.stringify({
						error: status === 403 ? 'recent_authentication_required' : 'untrusted message'
					}),
					{ status }
				)
			);
			const result = await keyApi(fetcher).create(creation);
			expect(result.kind).not.toBe('created');
			expect(JSON.stringify(result)).not.toContain('untrusted');
			expect(fetcher).toHaveBeenCalledOnce();
		}
		for (const value of [
			{},
			{ key: record, secret: 'bad' },
			{ key: { ...record, id: 'bad' }, secret: `dk_${'ab'.repeat(32)}` }
		]) {
			const fetcher = vi
				.fn()
				.mockResolvedValue(new Response(JSON.stringify(value), { status: 201 }));
			expect((await keyApi(fetcher).create(creation)).kind).toBe('uncertain');
		}
		const broken = vi.fn().mockRejectedValue(new Error('sensitive detail'));
		expect((await keyApi(broken).create(creation)).kind).toBe('uncertain');
		expect((await keyApi(broken).list()).kind).toBe('unavailable');
	});
});
