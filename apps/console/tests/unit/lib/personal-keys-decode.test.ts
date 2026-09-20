import { it, expect } from 'vitest';
import {
	decodeCreated,
	decodeKey,
	decodeOptions,
	decodePage
} from '../../../src/lib/personal-keys-decode';
import { key, options, secret, id } from './key-fixtures';
it('validates the entire metadata boundary and strips unknown secret material', () => {
	expect(decodeKey({ ...key, expires_ms: 2000, secret: 'discard' })).toEqual({
		...key,
		expires_ms: 2000
	});
	expect(decodeCreated({ key, secret })).toEqual({ key, secret });
	expect(decodePage({ items: [key], next: id(2) })).toEqual({ items: [key], next: id(2) });
	expect(decodeOptions(options)).toEqual(options);
	expect(decodeOptions({ ...options, next: id(3) })).not.toBeNull();
	for (const v of [
		null,
		[],
		0,
		'secret',
		{},
		{ ...key, id: 'invalid' },
		{ ...key, name: '\n' },
		{ ...key, name: 'a'.repeat(101) },
		{ ...key, created_ms: -1 },
		{ ...key, created_ms: Infinity },
		{ ...key, expires_ms: key.created_ms },
		{ ...key, active: 1 },
		{ ...key, grants: [] },
		{ ...key, grants: [...key.grants, ...key.grants] },
		{ ...key, grants: [{ resource_id: id(3), capabilities: [] }] },
		{ ...key, grants: [{ resource_id: id(3), capabilities: [id(4), id(4)] }] },
		{ ...key, grants: [{ resource_id: 'bad', capabilities: [id(4)] }] }
	])
		expect(decodeKey(v)).toBeNull();
	for (const v of [
		null,
		{ items: null, next: null },
		{ items: [key, key], next: null },
		{ items: Array.from({ length: 26 }, () => key), next: null },
		{ items: [{ ...key, id: 'bad' }], next: null },
		{ items: [], next: 'bad' }
	])
		expect(decodePage(v)).toBeNull();
	for (const v of [null, { secret: 'bad', key }, { secret, key: { ...key, created_ms: 0.5 } }])
		expect(decodeCreated(v)).toBeNull();
	for (const v of [
		null,
		{ ...options, policy_revision: '09' },
		{ ...options, policy: { ...options.policy, default_days: 0 } },
		{ ...options, policy: { ...options.policy, default_days: 366 } },
		{ ...options, policy: { ...options.policy, maximum_days: 3651 } },
		{ ...options, policy: { ...options.policy, allow_never: 'true' } },
		{ ...options, items: null },
		{ ...options, items: [{ ...options.items[0], application_id: 'bad' }] },
		{
			...options,
			items: [
				{
					...options.items[0],
					capabilities: [{ ...options.items[0].capabilities[0], meaning: '' }]
				}
			]
		},
		{ ...options, items: [{ ...options.items[0], capabilities: [] }] }
	])
		expect(decodeOptions(v)).toBeNull();
});
