import { it, expect, vi } from 'vitest';
import { mediaApi, validImage, branding, decodeSettings } from '../../../src/lib/media';
it('bounds upload input and treats lost replies as uncertain without retrying', async () => {
	const file = new File(['bytes'], 'avatar.png', { type: 'image/png' });
	expect(validImage(file)).toBe(true);
	expect(validImage(new File([], 'x', { type: 'image/png' }))).toBe(false);
	expect(validImage(new File(['<svg/>'], 'x', { type: 'image/svg+xml' }))).toBe(false);
	expect(
		validImage(new File([new Uint8Array(4 * 1024 * 1024 + 1)], 'x', { type: 'image/jpeg' }))
	).toBe(false);
	const fetcher = vi
		.fn()
		.mockResolvedValueOnce(new Response('{"revision":"2"}'))
		.mockRejectedValueOnce(new Error('private message'));
	const api = mediaApi(fetcher);
	expect(await api.upload('/api/profiles/me/picture', '1', file)).toEqual({
		kind: 'ready',
		value: '2'
	});
	expect(fetcher.mock.calls[0][1]).toMatchObject({
		method: 'POST',
		credentials: 'same-origin',
		headers: { 'x-darkhorse-csrf': '1', 'x-darkhorse-revision': '1' },
		body: file
	});
	expect((await api.remove('/api/profiles/me/picture', '2')).kind).toBe('failed');
	expect(fetcher).toHaveBeenCalledTimes(2);
	expect((await api.upload('x', '0', new File([], 'x'))).kind).toBe('failed');
	expect(fetcher).toHaveBeenCalledTimes(2);
});
it('validates settings and uses only fixed image locations for public branding', async () => {
	const settings = {
		revision: '0',
		logo: true,
		background: false,
		storage_enabled: true,
		bucket: 'test'
	};
	expect(decodeSettings(settings)).toEqual(settings);
	expect(decodeSettings({ ...settings, revision: '01' })).toBeNull();
	expect(decodeSettings(null)).toBeNull();
	const fetcher = vi
		.fn()
		.mockResolvedValueOnce(new Response(JSON.stringify(settings)))
		.mockResolvedValueOnce(new Response('{}'))
		.mockRejectedValueOnce(new Error('secret'));
	const api = mediaApi(fetcher);
	expect(await api.settings()).toEqual({ kind: 'ready', value: settings });
	expect((await api.settings()).kind).toBe('failed');
	expect((await api.settings()).kind).toBe('failed');
	expect(
		await branding(
			vi
				.fn()
				.mockResolvedValue(
					new Response('{"logo":true,"background":true,"url":"https://evil.example"}')
				)
		)
	).toEqual({ logo: true, background: true });
	expect(await branding(vi.fn().mockRejectedValue(new Error()))).toEqual({
		logo: false,
		background: false
	});
	expect(await branding(vi.fn().mockResolvedValue(new Response('{}', { status: 503 })))).toEqual({
		logo: false,
		background: false
	});
	for (const status of [401, 403, 409, 400, 413, 503]) {
		const result = await mediaApi(
			vi.fn().mockResolvedValue(new Response('untrusted', { status }))
		).remove('x', '0');
		expect(result.kind).toBe('failed');
		expect(JSON.stringify(result)).not.toContain('untrusted');
	}
});
