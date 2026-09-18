export type PendingAuthorization = {
	kind: 'pending';
	request_id: string;
	client_name: string;
	scopes: string[];
	resource: string | null;
	status: 'login' | 'consent' | 'ready';
};
export type AuthorizationState =
	PendingAuthorization | { kind: 'unavailable' } | { kind: 'redirect'; url: string };
export async function loadAuthorization(fetcher: typeof fetch): Promise<AuthorizationState> {
	return request(fetcher, '/api/authorization', { method: 'GET' });
}
export async function decideAuthorization(
	fetcher: typeof fetch,
	request_id: string,
	decision: 'approve' | 'deny'
): Promise<AuthorizationState> {
	return request(fetcher, '/api/authorization/decision', {
		method: 'POST',
		headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' },
		body: JSON.stringify({ request_id, decision })
	});
}
async function request(
	fetcher: typeof fetch,
	path: string,
	init: RequestInit
): Promise<AuthorizationState> {
	try {
		const response = await fetcher(path, {
			...init,
			credentials: 'same-origin',
			cache: 'no-store'
		});
		return response.ok ? project(await response.json()) : { kind: 'unavailable' };
	} catch {
		return { kind: 'unavailable' };
	}
}
function project(value: unknown): AuthorizationState {
	if (typeof value !== 'object' || value === null) return { kind: 'unavailable' };
	if ('redirect' in value && typeof value.redirect === 'string') {
		const url = new URL(value.redirect);
		if (url.protocol === 'https:' && !url.username && !url.password && !url.hash)
			return { kind: 'redirect', url: url.href };
	}
	if (
		'request_id' in value &&
		typeof value.request_id === 'string' &&
		/^[a-f0-9]{64}$/.test(value.request_id) &&
		'client_name' in value &&
		typeof value.client_name === 'string' &&
		value.client_name.length > 0 &&
		value.client_name.length <= 400 &&
		'scopes' in value &&
		Array.isArray(value.scopes) &&
		value.scopes.length > 0 &&
		value.scopes.length <= 32 &&
		value.scopes.every((s: unknown) => typeof s === 'string' && s.length <= 100) &&
		'resource' in value &&
		(value.resource === null || typeof value.resource === 'string') &&
		'status' in value &&
		(value.status === 'login' || value.status === 'consent' || value.status === 'ready')
	) {
		return {
			kind: 'pending',
			request_id: value.request_id,
			client_name: value.client_name,
			scopes: value.scopes,
			resource: value.resource,
			status: value.status
		};
	}
	return { kind: 'unavailable' };
}
