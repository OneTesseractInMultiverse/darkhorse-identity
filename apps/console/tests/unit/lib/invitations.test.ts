import { describe, it, expect, vi } from 'vitest';
import { invitationToken, acceptInvitation } from '$lib/invitations';
describe('invitation proof boundary', () => {
	it('accepts only the complete invitation fragment', () => {
		const token = `iv1_${'a'.repeat(64)}`;
		expect(invitationToken(`#token=${token}`)).toBe(token);
		for (const fragment of [
			'',
			`#token=${token}\n`,
			`#token=${token}&next=https://evil.example`,
			`#token=${token.toUpperCase()}`,
			`#token=ev1_${'a'.repeat(64)}`
		])
			expect(invitationToken(fragment)).toBeUndefined();
	});
	it('posts secrets only in a bounded same-origin request and preserves uncertainty', async () => {
		const input = {
			token: 'proof',
			email: 'new@example.com',
			first_name: 'New',
			last_name: 'Person',
			password: 'a long password'
		};
		for (const [status, expected] of [
			[200, 'ok'],
			[400, 'invalid'],
			[429, 'limited'],
			[404, 'unavailable'],
			[503, 'unavailable']
		] as const) {
			const fetcher = vi
				.fn<typeof fetch>()
				.mockResolvedValue(new Response('{"ok":true}', { status }));
			expect(await acceptInvitation(fetcher, input)).toBe(expected);
			expect(fetcher).toHaveBeenCalledWith(
				'/api/invitations/accept',
				expect.objectContaining({
					method: 'POST',
					credentials: 'same-origin',
					cache: 'no-store',
					headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' },
					body: JSON.stringify(input)
				})
			);
		}
		for (const body of ['{}', 'null', 'broken'])
			expect(await acceptInvitation(vi.fn().mockResolvedValue(new Response(body)), input)).toBe(
				'unavailable'
			);
		expect(await acceptInvitation(vi.fn().mockRejectedValue(new Error()), input)).toBe(
			'unavailable'
		);
	});
});
