export type Item = {
	kind: 'application' | 'client' | 'resource' | 'scope' | 'role' | 'capability';
	id: string;
	name: string;
	active?: boolean;
	revision?: string;
	application_id?: string;
	resource_id?: string;
	owner_id?: string;
	owner_email?: string;
	meaning?: string;
	audience?: string;
	redirect_uris?: string[];
	resource_ids?: string[];
	scope_ids?: string[];
	refresh_tokens?: boolean;
	secrets?: { id: string; created_ms: number; expires_ms: number | null }[];
};
export type Page = { items: Item[]; next: string | null; policy_revision: string };
export type View = {
	item: Item;
	applications: Item[];
	capabilities: Item[];
	policy_revision: string;
};
export type Registered = { record: Item; client_secret?: string };
export function object(v: unknown): v is Record<string, unknown> {
	return typeof v === 'object' && v !== null && !Array.isArray(v);
}
export function reference(v: unknown): v is string {
	return (
		typeof v === 'string' &&
		/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(v) &&
		v !== '00000000-0000-0000-0000-000000000000'
	);
}
export function counter(v: unknown): v is string {
	return (
		typeof v === 'string' && /^(0|[1-9][0-9]{0,18})$/.test(v) && BigInt(v) <= 9223372036854775807n
	);
}
function text(v: unknown, max: number): v is string {
	return typeof v === 'string' && v.length > 0 && Array.from(v).length <= max;
}
function strings(v: unknown, max: number, check: (x: unknown) => boolean): v is string[] {
	return Array.isArray(v) && v.length <= max && v.every(check) && new Set(v).size === v.length;
}
export function item(v: unknown): Item | null {
	if (!object(v) || !reference(v.id) || !text(v.name, 200)) return null;
	const base = { id: v.id, name: v.name };
	switch (v.kind) {
		case 'application':
			return typeof v.active === 'boolean' &&
				counter(v.revision) &&
				reference(v.owner_id) &&
				text(v.owner_email, 254)
				? {
						...base,
						kind: v.kind,
						active: v.active,
						revision: v.revision,
						owner_id: v.owner_id,
						owner_email: v.owner_email
					}
				: null;
		case 'client': {
			if (!reference(v.application_id) || typeof v.active !== 'boolean' || !counter(v.revision))
				return null;
			const client: Item = {
				...base,
				kind: v.kind,
				application_id: v.application_id,
				active: v.active,
				revision: v.revision
			};
			if (v.redirect_uris === undefined) return client;
			if (
				v.token_endpoint_auth_method !== 'client_secret_basic' ||
				!strings(v.redirect_uris, 8, (x) => text(x, 2048)) ||
				!strings(v.resource_ids, 32, reference) ||
				!strings(v.scope_ids, 128, reference) ||
				typeof v.refresh_tokens !== 'boolean' ||
				!Array.isArray(v.secrets) ||
				v.secrets.length > 2
			)
				return null;
			const secrets = [];
			for (const secret of v.secrets) {
				if (
					!object(secret) ||
					!reference(secret.id) ||
					!Number.isSafeInteger(secret.created_ms) ||
					(secret.created_ms as number) < 0 ||
					!(
						secret.expires_ms === null ||
						(Number.isSafeInteger(secret.expires_ms) && (secret.expires_ms as number) >= 0)
					)
				)
					return null;
				secrets.push({
					id: secret.id,
					created_ms: secret.created_ms as number,
					expires_ms: secret.expires_ms as number | null
				});
			}
			if (new Set(secrets.map((s) => s.id)).size !== secrets.length) return null;
			return {
				...client,
				redirect_uris: v.redirect_uris,
				resource_ids: v.resource_ids,
				scope_ids: v.scope_ids,
				refresh_tokens: v.refresh_tokens,
				secrets
			};
		}
		case 'resource':
			return reference(v.application_id) && text(v.audience, 2048)
				? { ...base, kind: v.kind, application_id: v.application_id, audience: v.audience }
				: null;
		case 'scope':
			return reference(v.application_id) && reference(v.resource_id)
				? { ...base, kind: v.kind, application_id: v.application_id, resource_id: v.resource_id }
				: null;
		case 'capability':
			return typeof v.active === 'boolean' && text(v.meaning, 1000)
				? { ...base, kind: v.kind, active: v.active, meaning: v.meaning }
				: null;
		case 'role':
			return { ...base, kind: v.kind };
		default:
			return null;
	}
}
function items(v: unknown, max: number): Item[] | null {
	if (!Array.isArray(v) || v.length > max) return null;
	const result = v.map(item);
	return result.some((v) => !v) || new Set(result.map((v) => v?.id)).size !== result.length
		? null
		: (result as Item[]);
}
export function page(v: unknown): Page | null {
	if (!object(v) || !counter(v.policy_revision) || !(v.next === null || reference(v.next)))
		return null;
	const records = items(v.items, 100);
	return records ? { items: records, next: v.next, policy_revision: v.policy_revision } : null;
}
export function view(v: unknown): View | null {
	if (!object(v) || !counter(v.policy_revision)) return null;
	const record = item(v.item),
		applications = items(v.applications, 1000),
		capabilities = items(v.capabilities, 256);
	return record &&
		applications?.every((a) => a.kind === 'application') &&
		capabilities?.every((a) => a.kind === 'capability')
		? { item: record, applications, capabilities, policy_revision: v.policy_revision }
		: null;
}
export function registered(v: unknown): Registered | null {
	if (!object(v)) return null;
	const record = item(v.record);
	if (
		!record ||
		!(
			v.client_secret === undefined ||
			(record.kind === 'client' &&
				typeof v.client_secret === 'string' &&
				/^[0-9a-f]{64}$/.test(v.client_secret))
		)
	)
		return null;
	return {
		...{ record },
		...(typeof v.client_secret === 'string' ? { client_secret: v.client_secret } : {})
	};
}
export function written(v: unknown): { policy_revision: string } | null {
	return object(v) && counter(v.policy_revision) && object(v.target) && reference(v.target.id)
		? { policy_revision: v.policy_revision }
		: null;
}
