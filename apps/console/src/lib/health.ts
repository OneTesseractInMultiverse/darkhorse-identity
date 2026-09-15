export type Connection = 'ready' | 'unavailable';
export type HealthPort = () => Promise<Connection>;

export async function checkHealth(request: typeof fetch): Promise<Connection> {
	try {
		const response = await request('/health/live', {
			cache: 'no-store',
			signal: AbortSignal.timeout(5000)
		});
		const body: unknown = await response.json();
		return connectionFromResponse(response.ok, body);
	} catch {
		return 'unavailable';
	}
}

function connectionFromResponse(ok: boolean, body: unknown): Connection {
	return ok && typeof body === 'object' && body !== null && 'status' in body && body.status === 'ok'
		? 'ready'
		: 'unavailable';
}
