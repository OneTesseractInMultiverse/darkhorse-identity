import { vi } from 'vitest';
import type { Key, Options } from '../../../src/lib/personal-keys';
export const id = (n: number) => `00000000-0000-0000-0000-${n.toString(16).padStart(12, '0')}`;
export const key: Key = {
	id: id(1),
	name: 'Build worker',
	application_id: id(2),
	created_ms: 1000,
	expires_ms: null,
	active: true,
	grants: [{ resource_id: id(3), capabilities: [id(4)] }]
};
export const options: Options = {
	policy_revision: '9',
	policy: { default_days: 30, maximum_days: 365, allow_never: true },
	items: [
		{
			application_id: id(2),
			application_name: 'Finance',
			resource_id: id(3),
			resource_name: 'Invoices',
			capabilities: [
				{ id: id(4), key: 'invoices.read', meaning: 'Read invoices' },
				{ id: id(5), key: 'invoices.write', meaning: 'Change invoices' }
			]
		}
	],
	next: null
};
export const secret = `dk_${'ab'.repeat(32)}`;
export function api() {
	return {
		list: vi.fn().mockResolvedValue({ kind: 'ready', value: { items: [key], next: null } }),
		options: vi.fn().mockResolvedValue({ kind: 'ready', value: options }),
		create: vi.fn().mockResolvedValue({ kind: 'created', key, secret }),
		revoke: vi.fn().mockResolvedValue({ kind: 'revoked' })
	};
}
