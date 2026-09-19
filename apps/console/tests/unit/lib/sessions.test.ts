import { expect, it, vi } from 'vitest';
import { readSessions, terminateSession, formatTime } from '../../../src/lib/sessions';
const id = '00000000-0000-0000-0000-000000000001';
const page = {
	current: id,
	items: [{ id, created_ms: 1000, seen_ms: 2000, expires_ms: 3000, status: 'active' }],
	next: null
};
it('uses bounded public references and protected same-origin requests', async () => {
	const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(page)));
	expect(await readSessions(fetcher)).toEqual({ kind: 'ready', page });
	expect(fetcher).toHaveBeenCalledWith(
		'/api/security/sessions',
		expect.objectContaining({ credentials: 'same-origin', cache: 'no-store' })
	);
	fetcher.mockResolvedValue(new Response(JSON.stringify({ ...page, next: `1000:${id}` })));
	expect((await readSessions(fetcher, `1000:${id}`)).kind).toBe('ready');
	expect(fetcher.mock.lastCall?.[0]).toBe(
		`/api/security/sessions?after=${encodeURIComponent(`1000:${id}`)}`
	);
	fetcher.mockResolvedValue(new Response('{"current":true}'));
	expect(await terminateSession(fetcher, id)).toEqual({ kind: 'ended', current: true });
	expect(fetcher).toHaveBeenLastCalledWith(
		'/api/security/sessions/end',
		expect.objectContaining({
			method: 'POST',
			headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' },
			body: JSON.stringify({ session_id: id })
		})
	);
	expect(formatTime(1000)).toBe('1970-01-01 00:00:01 UTC');
});
it('rejects malformed or oversized projections and handles unavailable or lost outcomes', async () => {
	for (const value of [
		{},
		{ ...page, current: 'secret' },
		{ ...page, items: new Array(26).fill(page.items[0]) },
		{ ...page, items: [page.items[0], page.items[0]] },
		{ ...page, items: [{ ...page.items[0], created_ms: -1 }] },
		{ ...page, items: [{ ...page.items[0], seen_ms: Number.MAX_SAFE_INTEGER }] },
		{ ...page, items: [{ ...page.items[0], status: 'unknown' }] },
		{ ...page, next: 'secret' },
		null
	]) {
		expect(
			await readSessions(vi.fn().mockResolvedValue(new Response(JSON.stringify(value))))
		).toEqual({ kind: 'unavailable' });
	}
	for (const [status, kind] of [
		[401, 'signed-out'],
		[503, 'unavailable'],
		[404, 'unavailable']
	] as const) {
		const fetcher = vi.fn().mockResolvedValue(new Response('untrusted', { status }));
		expect(await readSessions(fetcher)).toEqual({ kind });
		expect(await terminateSession(fetcher, id)).toEqual({
			kind: status === 401 ? 'signed-out' : 'uncertain'
		});
	}
	for (const fetcher of [
		vi.fn().mockRejectedValue(new Error('private')),
		vi.fn().mockImplementation(() => Promise.resolve(new Response('bad'))),
		vi.fn().mockImplementation(() => Promise.resolve(new Response('{}')))
	]) {
		expect(await readSessions(fetcher)).toEqual({ kind: 'unavailable' });
		expect(await terminateSession(fetcher, id)).toEqual({ kind: 'uncertain' });
	}
});
