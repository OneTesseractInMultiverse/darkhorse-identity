export type SessionRecord = {
	id: string;
	created_ms: number;
	seen_ms: number;
	expires_ms: number;
	status: 'active' | 'inactive';
};
export type SessionPage = { current: string; items: SessionRecord[]; next: string | null };
export type SessionsState =
	{ kind: 'ready'; page: SessionPage } | { kind: 'signed-out' | 'unavailable' };
export type Termination =
	{ kind: 'ended'; current: boolean } | { kind: 'signed-out' | 'uncertain' };
export async function readSessions(fetcher: typeof fetch, after?: string): Promise<SessionsState> {
	try {
		const response = await fetcher(
			`/api/security/sessions${after ? `?after=${encodeURIComponent(after)}` : ''}`,
			{ method: 'GET', credentials: 'same-origin', cache: 'no-store' }
		);
		if (!response.ok) return { kind: response.status === 401 ? 'signed-out' : 'unavailable' };
		return decode(await response.json());
	} catch {
		return { kind: 'unavailable' };
	}
}
export async function terminateSession(
	fetcher: typeof fetch,
	sessionId: string
): Promise<Termination> {
	try {
		const response = await fetcher('/api/security/sessions/end', {
			method: 'POST',
			credentials: 'same-origin',
			cache: 'no-store',
			headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' },
			body: JSON.stringify({ session_id: sessionId })
		});
		if (!response.ok) return { kind: response.status === 401 ? 'signed-out' : 'uncertain' };
		return termination(await response.json());
	} catch {
		return { kind: 'uncertain' };
	}
}
function termination(value: unknown): Termination {
	if (object(value) && typeof value.current === 'boolean')
		return { kind: 'ended', current: value.current };
	return { kind: 'uncertain' };
}
function object(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null;
}
function reference(value: unknown): value is string {
	return (
		typeof value === 'string' &&
		/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value) &&
		value !== '00000000-0000-0000-0000-000000000000'
	);
}
function timestamp(value: unknown): value is number {
	return (
		typeof value === 'number' &&
		Number.isSafeInteger(value) &&
		value >= 0 &&
		value <= 8_640_000_000_000_000
	);
}
function record(value: unknown): SessionRecord | null {
	if (
		!object(value) ||
		!reference(value.id) ||
		!timestamp(value.created_ms) ||
		!timestamp(value.seen_ms) ||
		!timestamp(value.expires_ms) ||
		value.created_ms > value.seen_ms ||
		value.seen_ms > value.expires_ms ||
		(value.status !== 'active' && value.status !== 'inactive')
	)
		return null;
	return {
		id: value.id,
		created_ms: value.created_ms,
		seen_ms: value.seen_ms,
		expires_ms: value.expires_ms,
		status: value.status
	};
}
function continuation(value: unknown): value is string | null {
	if (value === null) return true;
	if (typeof value !== 'string' || value.length > 128) return false;
	const [time, id, ...rest] = value.split(':');
	return (
		rest.length === 0 && /^(0|[1-9][0-9]*)$/.test(time) && timestamp(Number(time)) && reference(id)
	);
}
function decode(value: unknown): SessionsState {
	if (
		!object(value) ||
		!reference(value.current) ||
		!Array.isArray(value.items) ||
		value.items.length > 25 ||
		!continuation(value.next)
	)
		return { kind: 'unavailable' };
	const items = value.items.map(record);
	if (items.some((item) => item === null)) return { kind: 'unavailable' };
	if (new Set(items.map((item) => item!.id)).size !== items.length) return { kind: 'unavailable' };
	return {
		kind: 'ready',
		page: { current: value.current, items: items as SessionRecord[], next: value.next }
	};
}
export function formatTime(value: number): string {
	return `${new Date(value).toISOString().slice(0, 19).replace('T', ' ')} UTC`;
}
