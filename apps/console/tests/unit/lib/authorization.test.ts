import { expect, it, vi } from 'vitest';
import { loadAuthorization, decideAuthorization } from '../../../src/lib/authorization';
const id = 'a'.repeat(64);
const pending = {
	status: 'ready',
	request_id: id,
	client_name: 'Calendar',
	scopes: ['openid'],
	resource: null,
	ui_locale: 'es'
};
const response = (value: unknown) => vi.fn().mockResolvedValue(new Response(JSON.stringify(value)));
it('binds a decision to both the page reference and reviewed request with same-origin CSRF', async () => {
	const fetcher = response(pending);
	const result = await decideAuthorization(fetcher, id, id, 'approve');
	expect(result).toMatchObject({ kind: 'pending', ui_locale: 'es' });
	const [path, init] = fetcher.mock.calls[0];
	expect(path).toBe(`/api/authorization/decision?request=${id}`);
	expect(JSON.parse(init.body)).toEqual({ request_id: id, decision: 'approve' });
	expect(init.headers['x-darkhorse-csrf']).toBe('1');
	expect(init.credentials).toBe('same-origin');
});
it('rejects mismatched references and unsupported hints before accepting presentation state', async () => {
	for (const reference of [null, '', '../secret', id.toUpperCase(), id + '&request=other']) {
		const fetcher = vi.fn();
		expect(await loadAuthorization(fetcher, reference)).toEqual({ kind: 'unavailable' });
		expect(fetcher).not.toHaveBeenCalled();
	}
	const fetcher = vi.fn();
	expect(await decideAuthorization(fetcher, id, 'b'.repeat(64), 'approve')).toEqual({
		kind: 'unavailable'
	});
	expect(fetcher).not.toHaveBeenCalled();
	for (const value of [
		{ ...pending, request_id: 'b'.repeat(64) },
		{ ...pending, ui_locale: '../../es' },
		{ ...pending, ui_locale: undefined }
	])
		expect(await loadAuthorization(response(value), id)).toEqual({ kind: 'unavailable' });
	const valid = response({ ...pending, ui_locale: null });
	expect(await loadAuthorization(valid, id)).toMatchObject({ kind: 'pending', ui_locale: null });
	expect(valid.mock.calls[0][0]).toBe(`/api/authorization?request=${id}`);
});
it('rejects unsafe redirects, malformed projections and dependency failures', async () => {
	for (const value of [
		{ redirect: 'javascript:alert(1)' },
		{ redirect: 'http://example.com' },
		{ status: 'consent' },
		null
	])
		expect(await loadAuthorization(response(value), id)).toEqual({ kind: 'unavailable' });
	expect(await loadAuthorization(vi.fn().mockRejectedValue(new Error('offline')), id)).toEqual({
		kind: 'unavailable'
	});
	expect(
		await loadAuthorization(vi.fn().mockResolvedValue(new Response('', { status: 400 })), id)
	).toEqual({ kind: 'unavailable' });
	expect(
		await loadAuthorization(
			response({ redirect: 'https://client.example/cb?error=access_denied' }),
			id
		)
	).toEqual({ kind: 'redirect', url: 'https://client.example/cb?error=access_denied' });
});
