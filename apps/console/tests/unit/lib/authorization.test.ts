import { expect, it, vi } from 'vitest';
import { loadAuthorization, decideAuthorization } from '../../../src/lib/authorization';
it('binds a decision to the reviewed request and uses same-origin CSRF protection', async () => {
	const fetcher = vi.fn().mockResolvedValue(
		new Response(
			JSON.stringify({
				status: 'ready',
				request_id: 'a'.repeat(64),
				client_name: 'Calendar',
				scopes: ['openid'],
				resource: null
			})
		)
	);
	const result = await decideAuthorization(fetcher, 'a'.repeat(64), 'approve');
	expect(result.kind).toBe('pending');
	const [path, init] = fetcher.mock.calls[0];
	expect(path).toBe('/api/authorization/decision');
	expect(JSON.parse(init.body)).toEqual({ request_id: 'a'.repeat(64), decision: 'approve' });
	expect(init.headers['x-darkhorse-csrf']).toBe('1');
	expect(init.credentials).toBe('same-origin');
});
it('rejects unsafe redirects, malformed projections and dependency failures', async () => {
	for (const value of [
		{ redirect: 'javascript:alert(1)' },
		{ redirect: 'http://example.com' },
		{ status: 'consent' },
		null
	]) {
		expect(
			await loadAuthorization(vi.fn().mockResolvedValue(new Response(JSON.stringify(value))))
		).toEqual({ kind: 'unavailable' });
	}
	expect(await loadAuthorization(vi.fn().mockRejectedValue(new Error('offline')))).toEqual({
		kind: 'unavailable'
	});
	expect(
		await loadAuthorization(vi.fn().mockResolvedValue(new Response('', { status: 400 })))
	).toEqual({ kind: 'unavailable' });
	expect(
		await loadAuthorization(
			vi
				.fn()
				.mockResolvedValue(
					new Response(
						JSON.stringify({ redirect: 'https://client.example/cb?error=access_denied' })
					)
				)
		)
	).toEqual({ kind: 'redirect', url: 'https://client.example/cb?error=access_denied' });
});
