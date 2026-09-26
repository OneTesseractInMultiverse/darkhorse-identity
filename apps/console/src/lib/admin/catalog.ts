import type { Read } from './directory';
import {
	item,
	page,
	view,
	registered,
	written,
	type Item,
	type Page,
	type View,
	type Registered
} from './catalog-decode';
export type { Item, Page, View, Registered } from './catalog-decode';
export type Kind = 'applications' | 'clients' | 'resources' | 'scopes' | 'roles' | 'capabilities';
export type Query = { search?: string; status?: string; after?: string; application_id?: string };
export type Command = Record<string, unknown> & { operation: string };
export type Failure =
	'signed-out' | 'forbidden' | 'unavailable' | 'recent' | 'conflict' | 'invalid' | 'uncertain';
export type Write<T> = { kind: 'saved'; data: T } | { kind: Failure };
export type CatalogApi = {
	list(kind: Kind, query?: Query): Promise<Read<Page>>;
	detail(target: Item): Promise<Read<Item | View>>;
	register(command: Command): Promise<Write<Registered>>;
	change(revision: string, command: Command): Promise<Write<{ policy_revision: string }>>;
};
export function parameters(query: Query = {}): string {
	const p = new URLSearchParams({ limit: '25' });
	for (const [key, value] of Object.entries(query)) if (value?.trim()) p.set(key, value.trim());
	return p.toString();
}
export function detailPath(target: Item): string {
	if (target.kind === 'application') return `/api/admin/console/applications/${target.id}`;
	if (target.kind === 'client')
		return `/api/admin/console/applications/${target.application_id}/clients/${target.id}`;
	const p = new URLSearchParams();
	if (target.kind === 'resource' || target.kind === 'scope')
		p.set('application_id', target.application_id!);
	if (target.kind === 'scope') p.set('resource_id', target.resource_id!);
	return `/api/admin/catalog/${target.kind === 'capability' ? 'capabilities' : target.kind + 's'}/${target.id}${p.size ? '?' + p : ''}`;
}
export function catalogApi(fetcher: typeof fetch): CatalogApi {
	return {
		list: (kind, query) => read(fetcher, `/api/admin/catalog/${kind}?${parameters(query)}`, page),
		detail: (target) =>
			read<Item | View>(
				fetcher,
				detailPath(target),
				target.kind === 'application' || target.kind === 'client' ? item : view
			),
		register: (command) => write(fetcher, '/api/admin/console/registration', command, registered),
		change: (revision, command) =>
			write(fetcher, '/api/admin/catalog', { policy_revision: revision, change: command }, written)
	};
}
async function read<T>(
	fetcher: typeof fetch,
	path: string,
	decode: (v: unknown) => T | null
): Promise<Read<T>> {
	try {
		const response = await fetcher(path, {
			method: 'GET',
			credentials: 'same-origin',
			cache: 'no-store'
		});
		if (!response.ok)
			return {
				kind:
					response.status === 401
						? 'signed-out'
						: response.status === 403
							? 'forbidden'
							: 'unavailable'
			};
		const data = decode(await response.json());
		return data ? { kind: 'ready', data } : { kind: 'unavailable' };
	} catch {
		return { kind: 'unavailable' };
	}
}
async function write<T>(
	fetcher: typeof fetch,
	path: string,
	body: unknown,
	decode: (v: unknown) => T | null
): Promise<Write<T>> {
	try {
		const response = await fetcher(path, {
			method: 'POST',
			credentials: 'same-origin',
			cache: 'no-store',
			headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' },
			body: JSON.stringify(body)
		});
		const value: unknown = await response.json();
		return result(response.status, value, decode);
	} catch {
		return { kind: 'uncertain' };
	}
}
function result<T>(status: number, value: unknown, decode: (v: unknown) => T | null): Write<T> {
	if (status === 200) {
		const data = decode(value);
		return data ? { kind: 'saved', data } : { kind: 'uncertain' };
	}
	if (status === 401) return { kind: 'signed-out' };
	if (status === 403)
		return {
			kind:
				typeof value === 'object' &&
				value !== null &&
				'error' in value &&
				value.error === 'recent_authentication_required'
					? 'recent'
					: 'forbidden'
		};
	if (status === 409) return { kind: 'conflict' };
	if (status === 400) return { kind: 'invalid' };
	return { kind: 'uncertain' };
}
