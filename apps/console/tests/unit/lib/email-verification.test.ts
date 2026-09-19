import { describe, it, expect, vi } from 'vitest';
import { fragmentToken, emailStatus, requestEmail, confirmEmail } from '$lib/email-verification';
describe('email verification transport', () => {
	it('accepts only the complete canonical fragment and no redirect input', () => {
		const token = `ev1_${'a'.repeat(64)}`;
		expect(fragmentToken(`#token=${token}`)).toBe(token);
		for (const fragment of [
			'',
			`#token=${token}&redirect=x`,
			`#token=${token.toUpperCase()}`,
			'#token=short',
			`#token=${token}\n`
		])
			expect(fragmentToken(fragment)).toBeUndefined();
	});
	it('reads bounded typed status without exposing any proof', async () => {
		for (const [status, body, kind] of [
			[200, { email: 'one@example.com', verified: true }, 'ready'],
			[401, {}, 'signed-out'],
			[404, {}, 'disabled'],
			[503, {}, 'unavailable'],
			[200, { email: '', verified: true }, 'unavailable'],
			[200, { email: 'x'.repeat(255), verified: false }, 'unavailable'],
			[200, { email: 'a', verified: 'true' }, 'unavailable'],
			[200, null, 'unavailable']
		] as const) {
			const fetcher = vi
				.fn<typeof fetch>()
				.mockResolvedValue(new Response(JSON.stringify(body), { status }));
			expect((await emailStatus(fetcher)).kind).toBe(kind);
			expect(fetcher).toHaveBeenCalledWith('/api/security/email', {
				credentials: 'same-origin',
				cache: 'no-store'
			});
		}
		expect(await emailStatus(vi.fn().mockRejectedValue(new Error()))).toEqual({
			kind: 'unavailable'
		});
	});
	it('uses same-origin CSRF POST bodies and preserves uncertain outcomes', async () => {
		for (const [status, expected] of [
			[200, 'ok'],
			[400, 'invalid'],
			[401, 'signed-out'],
			[429, 'limited'],
			[503, 'unavailable']
		] as const) {
			const fetcher = vi
				.fn<typeof fetch>()
				.mockImplementation(async () => new Response('{"ok":true}', { status }));
			expect(await requestEmail(fetcher)).toBe(expected);
			expect(await confirmEmail(fetcher, 'proof')).toBe(expected);
			expect(fetcher).toHaveBeenLastCalledWith(
				'/api/security/email/confirm',
				expect.objectContaining({
					method: 'POST',
					body: '{"token":"proof"}',
					credentials: 'same-origin',
					cache: 'no-store',
					headers: expect.objectContaining({ 'x-darkhorse-csrf': '1' })
				})
			);
		}
		for (const body of ['{}', 'null', 'invalid'])
			expect(await requestEmail(vi.fn().mockResolvedValue(new Response(body)))).toBe('unavailable');
		expect(await requestEmail(vi.fn().mockRejectedValue(new Error()))).toBe('unavailable');
	});
});
