import type { Locale } from './i18n/locale';
export type PendingAuthorization = {
	kind: 'pending';
	request_id: string;
	client_name: string;
	scopes: string[];
	resource: string | null;
	status: 'login' | 'consent' | 'ready';
	ui_locale: Locale | null;
};
export type AuthorizationState =
	PendingAuthorization | { kind: 'unavailable' } | { kind: 'redirect'; url: string };
export async function loadAuthorization(
	fetcher: typeof fetch,
	reference: string | null
): Promise<AuthorizationState> {
	if (!validReference(reference)) return { kind: 'unavailable' };
	return request(fetcher, `/api/authorization?request=${reference}`, { method: 'GET' }, reference);
}
export async function decideAuthorization(
	fetcher: typeof fetch,
	reference: string | null,
	request_id: string,
	decision: 'approve' | 'deny'
): Promise<AuthorizationState> {
	if (!validReference(reference) || reference !== request_id) return { kind: 'unavailable' };
	return request(
		fetcher,
		`/api/authorization/decision?request=${reference}`,
		{
			method: 'POST',
			headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' },
			body: JSON.stringify({ request_id, decision })
		},
		reference
	);
}
async function request(
	fetcher: typeof fetch,
	path: string,
	init: RequestInit,
	reference: string
): Promise<AuthorizationState> {
	try {
		const response = await fetcher(path, {
			...init,
			credentials: 'same-origin',
			cache: 'no-store'
		});
		const result = response.ok
			? project(await response.json())
			: ({ kind: 'unavailable' } as const);
		return result.kind === 'pending' && result.request_id !== reference
			? { kind: 'unavailable' }
			: result;
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
		'ui_locale' in value &&
		(value.ui_locale === null || value.ui_locale === 'en' || value.ui_locale === 'es') &&
		'status' in value &&
		(value.status === 'login' || value.status === 'consent' || value.status === 'ready')
	) {
		return {
			kind: 'pending',
			request_id: value.request_id,
			client_name: value.client_name,
			scopes: value.scopes,
			resource: value.resource,
			status: value.status,
			ui_locale: value.ui_locale
		};
	}
	return { kind: 'unavailable' };
}

function validReference(reference: string | null): reference is string {
	return typeof reference === 'string' && /^[a-f0-9]{64}$/.test(reference);
}
