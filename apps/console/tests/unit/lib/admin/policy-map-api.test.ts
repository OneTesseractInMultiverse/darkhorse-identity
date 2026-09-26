import { describe, expect, it, vi } from 'vitest';
import { readPolicyMap } from '../../../../src/lib/admin/policy-map-api';

const applicationId = '10000000-0000-4000-8000-000000000001';
function graph() {
	return {
		application: { id: applicationId, name: 'Console', active: true },
		policy_revision: '0',
		complete: true,
		nodes: [
			{
				id: `application:${applicationId}`,
				type: 'application',
				identifier: applicationId,
				name: 'Console',
				active: true
			}
		],
		edges: []
	};
}

describe('readPolicyMap', () => {
	it('makes a same-origin no-store read and validates the graph', async () => {
		const fetcher = vi
			.fn()
			.mockResolvedValue(new Response(JSON.stringify(graph()), { status: 200 }));
		const result = await readPolicyMap(fetcher, applicationId);
		expect(result.kind).toBe('ready');
		expect(fetcher).toHaveBeenCalledWith(
			`/api/admin/console/applications/${applicationId}/policy-map`,
			expect.objectContaining({ method: 'GET', credentials: 'same-origin', cache: 'no-store' })
		);
	});

	it.each([
		[401, 'signed-out'],
		[403, 'forbidden'],
		[413, 'too-large'],
		[503, 'unavailable']
	] as const)('maps HTTP %s to the explicit read result', async (status, kind) => {
		const result = await readPolicyMap(
			vi.fn().mockResolvedValue(new Response('', { status })),
			applicationId
		);
		expect(result).toEqual({ kind });
	});

	it('fails closed on invalid JSON data and rejected reads', async () => {
		const invalid = await readPolicyMap(
			vi
				.fn()
				.mockResolvedValue(
					new Response(JSON.stringify({ ...graph(), complete: false }), { status: 200 })
				),
			applicationId
		);
		expect(invalid).toEqual({ kind: 'unavailable' });
		const rejected = await readPolicyMap(
			vi.fn().mockRejectedValue(new Error('offline')),
			applicationId
		);
		expect(rejected).toEqual({ kind: 'unavailable' });
	});

	it('treats canceled reads as aborted without exposing their stale result', async () => {
		const controller = new AbortController();
		controller.abort();
		const result = await readPolicyMap(
			vi.fn().mockRejectedValue(new DOMException('Aborted', 'AbortError')),
			applicationId,
			controller.signal
		);
		expect(result).toEqual({ kind: 'aborted' });
	});

	it('treats any failure from an already canceled request as an abort', async () => {
		const controller = new AbortController();
		controller.abort();
		const result = await readPolicyMap(
			vi.fn().mockRejectedValue(new Error('transport closed after cancellation')),
			applicationId,
			controller.signal
		);
		expect(result).toEqual({ kind: 'aborted' });
	});

	it('recognizes a fetch AbortError even before the signal reports cancellation', async () => {
		const result = await readPolicyMap(
			vi.fn().mockRejectedValue(new DOMException('Aborted', 'AbortError')),
			applicationId
		);
		expect(result).toEqual({ kind: 'aborted' });
	});
});
