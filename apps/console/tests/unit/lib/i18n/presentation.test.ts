import { expect, it, vi } from 'vitest';
import { presentation } from '../../../../src/lib/i18n/presentation';
it('reads a bounded public default without credentials and ignores failures or unknown values', async () => {
	const fetcher = vi.fn().mockResolvedValue(new Response('{"default_locale":"es"}'));
	expect(await presentation(fetcher)).toBe('es');
	expect(fetcher).toHaveBeenCalledWith('/api/presentation', {
		credentials: 'omit',
		cache: 'no-store',
		redirect: 'error'
	});
	for (const response of [
		new Response('{}'),
		new Response('{"default_locale":"fr"}'),
		new Response('{"default_locale":"es-CR"}'),
		new Response('bad'),
		new Response('x', { status: 503 })
	])
		expect(await presentation(vi.fn().mockResolvedValue(response))).toBeUndefined();
	expect(await presentation(vi.fn().mockRejectedValue(new Error('secret')))).toBeUndefined();
});
