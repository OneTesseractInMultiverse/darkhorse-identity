import { describe, expect, it, vi } from 'vitest';
import { checkHealth } from '../../../src/lib/health';

describe('health boundary', () => {
	it('checks the same origin without caching', async () => {
		const request = vi.fn().mockResolvedValue(new Response('{"status":"ok"}'));
		expect(await checkHealth(request)).toBe('ready');
		expect(request).toHaveBeenCalledWith('/health/live', {
			cache: 'no-store',
			signal: expect.any(AbortSignal)
		});
	});

	it.each([
		new Response('{"status":"ok"}', { status: 503 }),
		new Response('null'),
		new Response('42'),
		new Response('{}'),
		new Response('broken', { status: 503 }),
		new Response('bad json'),
		new Response('{"status":"unknown"}')
	])('reports bad responses as unavailable', async (response) => {
		expect(await checkHealth(vi.fn().mockResolvedValue(response))).toBe('unavailable');
	});

	it('reports network failure without exposing diagnostic text', async () => {
		expect(await checkHealth(vi.fn().mockRejectedValue(new Error('private upstream detail')))).toBe(
			'unavailable'
		);
	});
});
