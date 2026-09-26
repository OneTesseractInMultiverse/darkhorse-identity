export type User = {
	id: string;
	email: string;
	first_name: string;
	last_name: string;
	active: boolean;
	administrator: boolean;
	email_verified: boolean;
	revision: string;
};
export type Query = { search: string; status: '' | 'active' | 'inactive'; after?: string };
export type Page = { actor: string; items: User[]; next: string | null };
export type Application = { id: string; name: string; active: boolean };
export type Role = { id: string; name: string; assigned: boolean };
export type Access = {
	user: User;
	policy_revision: string;
	applications: Application[];
	selected: string | null;
	roles: Role[];
};
export type Change =
	| { kind: 'names'; first_name: string; last_name: string }
	| { kind: 'status'; active: boolean }
	| {
			kind: 'role';
			application_id: string;
			role_id: string;
			assigned: boolean;
			policy_revision: string;
	  };
export type Failure = 'signed-out' | 'forbidden' | 'unavailable';
export type Read<T> = { kind: 'ready'; data: T } | { kind: Failure };
export type Write =
	| { kind: 'saved'; user: User }
	| {
			kind:
				| 'signed-out'
				| 'forbidden'
				| 'recent'
				| 'conflict'
				| 'last-administrator'
				| 'invalid'
				| 'uncertain';
	  };
export type DirectoryApi = {
	list(query: Query): Promise<Read<Page>>;
	user(id: string): Promise<Read<User>>;
	access(id: string, application?: string): Promise<Read<Access>>;
	update(user: User, change: Change): Promise<Write>;
};
export function directoryApi(fetcher: typeof fetch): DirectoryApi {
	return {
		list: (query) => read(fetcher, `/api/admin/users?${parameters(query)}`, page),
		user: (id) => read(fetcher, `/api/admin/users/${encodeURIComponent(id)}`, user),
		access: (id, application) =>
			read(
				fetcher,
				`/api/admin/users/${encodeURIComponent(id)}/access${application ? `?application=${encodeURIComponent(application)}` : ''}`,
				access
			),
		update: (user, change) => update(fetcher, user, change)
	};
}
function parameters(query: Query): string {
	const result = new URLSearchParams({ limit: '25' });
	if (query.search.trim()) result.set('search', query.search.trim());
	if (query.status) result.set('status', query.status);
	if (query.after) result.set('after', query.after);
	return result.toString();
}
async function read<T>(
	fetcher: typeof fetch,
	path: string,
	decode: (value: unknown) => T | null
): Promise<Read<T>> {
	try {
		const response = await fetcher(path, {
			method: 'GET',
			credentials: 'same-origin',
			cache: 'no-store'
		});
		if (!response.ok) return { kind: readFailure(response.status) };
		return decoded(decode(await response.json()));
	} catch {
		return { kind: 'unavailable' };
	}
}
function readFailure(status: number): Failure {
	return status === 401 ? 'signed-out' : status === 403 ? 'forbidden' : 'unavailable';
}
function decoded<T>(value: T | null): Read<T> {
	return value === null ? { kind: 'unavailable' } : { kind: 'ready', data: value };
}
async function update(fetcher: typeof fetch, target: User, change: Change): Promise<Write> {
	try {
		const response = await fetcher(`/api/admin/users/${encodeURIComponent(target.id)}`, {
			method: 'POST',
			credentials: 'same-origin',
			cache: 'no-store',
			headers: { 'content-type': 'application/json', 'x-darkhorse-csrf': '1' },
			body: JSON.stringify({ revision: target.revision, change })
		});
		const body: unknown = await response.json();
		return written(response.status, body);
	} catch {
		return { kind: 'uncertain' };
	}
}
function written(status: number, body: unknown): Write {
	if (status === 200) {
		const record = user(body);
		return record ? { kind: 'saved', user: record } : { kind: 'uncertain' };
	}
	if (status === 401) return { kind: 'signed-out' };
	const error = object(body) ? body.error : undefined;
	if (status === 403)
		return { kind: error === 'recent_authentication_required' ? 'recent' : 'forbidden' };
	if (status === 409)
		return { kind: error === 'last_administrator' ? 'last-administrator' : 'conflict' };
	if (status === 400) return { kind: 'invalid' };
	return { kind: 'uncertain' };
}
function object(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function reference(value: unknown): value is string {
	return (
		typeof value === 'string' &&
		/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value) &&
		value !== '00000000-0000-0000-0000-000000000000'
	);
}
function counter(value: unknown): value is string {
	return (
		typeof value === 'string' &&
		/^(0|[1-9][0-9]{0,18})$/.test(value) &&
		BigInt(value) <= 9223372036854775807n
	);
}
function text(value: unknown, max: number): value is string {
	return typeof value === 'string' && value.length > 0 && Array.from(value).length <= max;
}
function user(value: unknown): User | null {
	if (
		!object(value) ||
		!reference(value.id) ||
		!text(value.email, 254) ||
		!text(value.first_name, 100) ||
		!text(value.last_name, 100) ||
		!counter(value.revision) ||
		typeof value.active !== 'boolean' ||
		typeof value.administrator !== 'boolean' ||
		typeof value.email_verified !== 'boolean'
	)
		return null;
	return {
		id: value.id,
		email: value.email,
		first_name: value.first_name,
		last_name: value.last_name,
		revision: value.revision,
		active: value.active,
		administrator: value.administrator,
		email_verified: value.email_verified
	};
}
function page(value: unknown): Page | null {
	if (
		!object(value) ||
		!reference(value.actor) ||
		!Array.isArray(value.items) ||
		value.items.length > 100 ||
		!(value.next === null || reference(value.next))
	)
		return null;
	const items = value.items.map(user);
	if (
		items.some((item) => item === null) ||
		new Set(items.map((item) => item?.id)).size !== items.length
	)
		return null;
	return { actor: value.actor, items: items as User[], next: value.next };
}
function application(value: unknown): Application | null {
	if (
		!object(value) ||
		!reference(value.id) ||
		!text(value.name, 100) ||
		typeof value.active !== 'boolean'
	)
		return null;
	return { id: value.id, name: value.name, active: value.active };
}
function role(value: unknown): Role | null {
	if (
		!object(value) ||
		!reference(value.id) ||
		!text(value.name, 100) ||
		typeof value.assigned !== 'boolean'
	)
		return null;
	return { id: value.id, name: value.name, assigned: value.assigned };
}
function access(value: unknown): Access | null {
	if (
		!object(value) ||
		!counter(value.policy_revision) ||
		!Array.isArray(value.applications) ||
		value.applications.length > 100 ||
		!Array.isArray(value.roles) ||
		value.roles.length > 128 ||
		!(value.selected === null || reference(value.selected))
	)
		return null;
	const target = user(value.user),
		applications = value.applications.map(application),
		roles = value.roles.map(role);
	if (
		!target ||
		applications.some((a) => a === null) ||
		roles.some((r) => r === null) ||
		new Set(applications.map((a) => a?.id)).size !== applications.length ||
		new Set(roles.map((r) => r?.id)).size !== roles.length ||
		(value.selected !== null && !applications.some((a) => a?.id === value.selected)) ||
		(value.selected === null && roles.length > 0)
	)
		return null;
	return {
		user: target,
		policy_revision: value.policy_revision,
		applications: applications as Application[],
		selected: value.selected,
		roles: roles as Role[]
	};
}
