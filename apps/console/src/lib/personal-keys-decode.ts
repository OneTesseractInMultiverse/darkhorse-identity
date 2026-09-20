import { object, reference, counter } from './admin/catalog-decode';
import type { Key, Page, Options, Eligible, Grant } from './personal-keys';
function text(v: unknown, max: number): v is string {
	return typeof v === 'string' && v.length > 0 && Array.from(v).length <= max && !/\p{Cc}/u.test(v);
}
function time(v: unknown): v is number {
	return typeof v === 'number' && Number.isSafeInteger(v) && v >= 0 && v <= 8640000000000000;
}
function refs(v: unknown): v is string[] {
	return (
		Array.isArray(v) &&
		v.length > 0 &&
		v.length <= 256 &&
		v.every(reference) &&
		new Set(v).size === v.length
	);
}
function array<T>(
	v: unknown,
	max: number,
	decode: (v: unknown) => T | null,
	identity: (v: T) => string
): T[] | null {
	if (!Array.isArray(v) || v.length > max) return null;
	const items = v.map(decode);
	if (items.some((item) => item === null)) return null;
	const result = items as T[];
	return new Set(result.map(identity)).size === result.length ? result : null;
}
function grant(v: unknown): Grant | null {
	return object(v) && reference(v.resource_id) && refs(v.capabilities)
		? { resource_id: v.resource_id, capabilities: [...v.capabilities] }
		: null;
}
export function decodeKey(v: unknown): Key | null {
	if (
		!object(v) ||
		!reference(v.id) ||
		!text(v.name, 100) ||
		!reference(v.application_id) ||
		!time(v.created_ms) ||
		!(v.expires_ms === null || (time(v.expires_ms) && v.expires_ms > v.created_ms)) ||
		typeof v.active !== 'boolean'
	)
		return null;
	const grants = array(v.grants, 16, grant, (g) => g.resource_id);
	if (!grants?.length) return null;
	return {
		id: v.id,
		name: v.name,
		application_id: v.application_id,
		created_ms: v.created_ms,
		expires_ms: v.expires_ms,
		active: v.active,
		grants
	};
}
export function decodePage(v: unknown): Page | null {
	if (!object(v) || !(v.next === null || reference(v.next))) return null;
	const items = array(v.items, 25, decodeKey, (k) => k.id);
	return items ? { items, next: v.next } : null;
}
function capability(v: unknown): Eligible['capabilities'][number] | null {
	return object(v) && reference(v.id) && text(v.key, 200) && text(v.meaning, 1000)
		? { id: v.id, key: v.key, meaning: v.meaning }
		: null;
}
function eligible(v: unknown): Eligible | null {
	if (
		!object(v) ||
		!reference(v.application_id) ||
		!reference(v.resource_id) ||
		!text(v.application_name, 100) ||
		!text(v.resource_name, 100)
	)
		return null;
	const capabilities = array(v.capabilities, 256, capability, (c) => c.id);
	return capabilities?.length
		? {
				application_id: v.application_id,
				application_name: v.application_name,
				resource_id: v.resource_id,
				resource_name: v.resource_name,
				capabilities
			}
		: null;
}
export function decodeOptions(v: unknown): Options | null {
	if (
		!object(v) ||
		!counter(v.policy_revision) ||
		!(v.next === null || reference(v.next)) ||
		!object(v.policy)
	)
		return null;
	const p = v.policy;
	if (
		typeof p.default_days !== 'number' ||
		!Number.isInteger(p.default_days) ||
		typeof p.maximum_days !== 'number' ||
		!Number.isInteger(p.maximum_days) ||
		p.default_days < 1 ||
		p.default_days > p.maximum_days ||
		p.maximum_days > 3650 ||
		typeof p.allow_never !== 'boolean'
	)
		return null;
	const items = array(v.items, 25, eligible, (r) => r.resource_id);
	return items
		? {
				items,
				next: v.next,
				policy_revision: v.policy_revision,
				policy: {
					default_days: p.default_days,
					maximum_days: p.maximum_days,
					allow_never: p.allow_never
				}
			}
		: null;
}
export function decodeCreated(v: unknown): { key: Key; secret: string } | null {
	if (!object(v) || typeof v.secret !== 'string' || !/^dk_[0-9a-f]{64}$/.test(v.secret))
		return null;
	const key = decodeKey(v.key);
	return key ? { key, secret: v.secret } : null;
}
