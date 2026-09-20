import { decodeCreated, decodeOptions, decodePage } from './personal-keys-decode';
export type Grant = { resource_id: string; capabilities: string[] };
export type Key = {
	id: string;
	name: string;
	application_id: string;
	created_ms: number;
	expires_ms: number | null;
	active: boolean;
	grants: Grant[];
};
export type Page = { items: Key[]; next: string | null };
export type Eligible = {
	application_id: string;
	application_name: string;
	resource_id: string;
	resource_name: string;
	capabilities: { id: string; key: string; meaning: string }[];
};
export type Options = {
	policy_revision: string;
	policy: { default_days: number; maximum_days: number; allow_never: boolean };
	items: Eligible[];
	next: string | null;
};
export type Selection = { kind: 'all' } | { kind: 'subset'; capabilities: string[] };
export type Creation = {
	name: string;
	application_id: string;
	policy_revision: string;
	expiration: { kind: 'default' | 'never' } | { kind: 'days'; days: number };
	grants: { resource_id: string; selection: Selection }[];
};
export type Failure = {
	kind:
		| 'signed-out'
		| 'reauthenticate'
		| 'denied'
		| 'changed'
		| 'limited'
		| 'invalid'
		| 'uncertain'
		| 'unavailable';
};
export type Read<T> = { kind: 'ready'; value: T } | Failure;
export type Created = { kind: 'created'; key: Key; secret: string } | Failure;
export type Revoked = { kind: 'revoked' } | Failure;
export type KeyApi = {
	list: (after?: string) => Promise<Read<Page>>;
	options: (after?: string) => Promise<Read<Options>>;
	create: (input: Creation) => Promise<Created>;
	revoke: (id: string) => Promise<Revoked>;
};
export function keyApi(fetcher: typeof fetch): KeyApi {
	async function read<T>(
		path: string,
		decode: (v: unknown) => T | null,
		after?: string
	): Promise<Read<T>> {
		try {
			const response = await fetcher(
				`/api/security/keys${path}${after ? `?after=${encodeURIComponent(after)}` : ''}`,
				{ credentials: 'same-origin', cache: 'no-store' }
			);
			if (!response.ok) return failure(response);
			const value = decode(await response.json());
			return value ? { kind: 'ready', value } : { kind: 'unavailable' };
		} catch {
			return { kind: 'unavailable' };
		}
	}
	async function write(path: string, body: unknown): Promise<Response> {
		return fetcher(`/api/security/keys${path}`, {
			method: 'POST',
			credentials: 'same-origin',
			cache: 'no-store',
			headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' },
			body: JSON.stringify(body)
		});
	}
	return {
		list: (after) => read('', decodePage, after),
		options: (after) => read('/options', decodeOptions, after),
		async create(input) {
			try {
				const response = await write('', input);
				if (response.status !== 201) return failure(response);
				const created = decodeCreated(await response.json());
				return created ? { kind: 'created', ...created } : { kind: 'uncertain' };
			} catch {
				return { kind: 'uncertain' };
			}
		},
		async revoke(id) {
			try {
				const response = await write('/revoke', { key_id: id });
				return response.status === 204 ? { kind: 'revoked' } : failure(response);
			} catch {
				return { kind: 'uncertain' };
			}
		}
	};
}
async function failure(response: Response): Promise<Failure> {
	if (response.status === 401) return { kind: 'signed-out' };
	if (response.status === 403) {
		try {
			const value = await response.json();
			if (value?.error === 'recent_authentication_required') return { kind: 'reauthenticate' };
		} catch {
			/* Only the fixed status is used when the body is malformed. */
		}
		return { kind: 'denied' };
	}
	if (response.status === 409) return { kind: 'changed' };
	if (response.status === 429) return { kind: 'limited' };
	if (response.status === 400) return { kind: 'invalid' };
	return { kind: 'uncertain' };
}
export function failureMessage(failure: Failure): string {
	switch (failure.kind) {
		case 'signed-out':
			return 'Your session has ended. Sign in to manage API keys.';
		case 'reauthenticate':
			return 'Sign in again before creating or revoking an API key. This requires a sign-in within five minutes.';
		case 'denied':
			return 'Your current access does not permit this request. Refresh permissions before continuing.';
		case 'changed':
			return 'Permissions changed. Refresh permissions and review your selection.';
		case 'limited':
			return 'Key issuance is limited to 100 live keys and 10 new keys per ten minutes.';
		case 'invalid':
			return 'The request was rejected. Refresh permissions and review the key details.';
		case 'unavailable':
			return 'API keys are temporarily unavailable. Refresh the list to try again.';
		case 'uncertain':
			return 'The change could not be confirmed. Refresh your key list before trying again. A lost secret cannot be retrieved; revoke that key and create a replacement.';
	}
}
