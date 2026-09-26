import { expect, it, vi } from 'vitest';
import { authenticate, currentSession, endSession } from '../../../src/lib/authentication';

it('uses same-origin cookie transport and explicit CSRF headers without storing secrets', async () => {
	const fetcher = vi
		.fn()
		.mockImplementation(async () => new Response(JSON.stringify({ name: 'Ada' })));
	expect(await authenticate(fetcher, 'a@example.com', 'test-only')).toEqual({
		kind: 'signed-in',
		name: 'Ada'
	});
	expect(fetcher).toHaveBeenCalledWith(
		'/api/auth/login',
		expect.objectContaining({
			method: 'POST',
			credentials: 'same-origin',
			headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' },
			body: JSON.stringify({ email: 'a@example.com', password: 'test-only' })
		})
	);
	expect(await currentSession(fetcher)).toEqual({ kind: 'signed-in', name: 'Ada' });
	fetcher.mockResolvedValue(new Response(null, { status: 204 }));
	expect(await endSession(fetcher)).toBe(true);
});
it('distinguishes generic denial, limits and unavailability without reflecting server content', async () => {
	for (const [status, kind] of [
		[401, 'signed-out'],
		[429, 'limited'],
		[503, 'unavailable'],
		[403, 'unavailable']
	] as const) {
		const fetcher = vi.fn().mockResolvedValue(new Response('secret', { status }));
		expect(await authenticate(fetcher, 'a@example.com', 'test')).toEqual({ kind });
		expect(await endSession(fetcher)).toBe(false);
	}
	for (const fetcher of [
		vi.fn().mockRejectedValue(new Error('secret')),
		vi.fn().mockResolvedValue(new Response('{}')),
		vi.fn().mockResolvedValue(new Response('bad'))
	]) {
		expect(await currentSession(fetcher)).toEqual({ kind: 'unavailable' });
		expect(await endSession(fetcher)).toBe(false);
	}
});
it('accepts only allowlisted account preferences in an authenticated session projection', async () => {
	for (const locale of ['en', 'es', null] as const) {
		const state = await currentSession(
			vi
				.fn()
				.mockResolvedValue(new Response(JSON.stringify({ name: 'Ada', preferred_locale: locale })))
		);
		expect(state).toEqual({ kind: 'signed-in', name: 'Ada', locale: locale ?? undefined });
	}
	for (const locale of ['es-CR', '../en', true, {}])
		expect(
			await currentSession(
				vi
					.fn()
					.mockResolvedValue(
						new Response(JSON.stringify({ name: 'Ada', preferred_locale: locale }))
					)
			)
		).toEqual({ kind: 'unavailable' });
});
