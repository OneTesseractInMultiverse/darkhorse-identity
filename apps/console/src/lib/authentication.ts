export type AuthState =
	{ kind: 'signed-in'; name: string } | { kind: 'signed-out' | 'limited' | 'unavailable' };
type Fetcher = typeof fetch;
const headers = { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' };
export async function authenticate(
	fetcher: Fetcher,
	email: string,
	password: string
): Promise<AuthState> {
	return request(fetcher, '/api/auth/login', {
		method: 'POST',
		headers,
		body: JSON.stringify({ email, password })
	});
}
export async function currentSession(fetcher: Fetcher): Promise<AuthState> {
	return request(fetcher, '/api/auth/session', { method: 'GET' });
}
export async function endSession(fetcher: Fetcher): Promise<boolean> {
	try {
		const response = await fetcher('/api/auth/logout', {
			method: 'POST',
			headers,
			credentials: 'same-origin',
			cache: 'no-store'
		});
		return response.status === 204;
	} catch {
		return false;
	}
}
async function request(fetcher: Fetcher, path: string, init: RequestInit): Promise<AuthState> {
	try {
		const response = await fetcher(path, {
			...init,
			credentials: 'same-origin',
			cache: 'no-store'
		});
		if (!response.ok) return failure(response.status);
		return profile(await response.json());
	} catch {
		return { kind: 'unavailable' };
	}
}
function failure(status: number): AuthState {
	if (status === 401) return { kind: 'signed-out' };
	if (status === 429) return { kind: 'limited' };
	return { kind: 'unavailable' };
}
function profile(value: unknown): AuthState {
	if (
		typeof value === 'object' &&
		value !== null &&
		'name' in value &&
		typeof value.name === 'string' &&
		value.name.length > 0 &&
		value.name.length <= 400
	) {
		return { kind: 'signed-in', name: value.name };
	}
	return { kind: 'unavailable' };
}
