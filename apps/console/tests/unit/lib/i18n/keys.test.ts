import { render, screen, fireEvent } from '@testing-library/svelte';
import { it, expect, vi } from 'vitest';
import { createLocalization } from '../../../../src/lib/i18n/state';
import { createFormatter } from '../../../../src/lib/i18n/format';
import { contract } from '../../../../src/lib/i18n/contract';
import Harness from './KeyHarness.svelte';
it('keeps exact resource and permission selections when a key editor changes language', async () => {
	const en = Object.entries(contract).map(([key, args]) => [
		key,
		[key, ...Object.keys(args).map((name) => `{${name}}`)].join(' ')
	]);
	const language = createLocalization(
		createFormatter(contract, { en, es: en.map(([key, text]) => [key, `ES ${text}`]) })
	);
	language.select('es');
	const resource = {
		application_id: 'app-id',
		application_name: 'Reports',
		resource_id: 'resource-id',
		resource_name: 'Private Reports',
		capabilities: [{ id: 'cap-id', key: 'reports:read', meaning: 'Read reports' }]
	};
	const properties = {
		read: vi.fn().mockResolvedValue({
			kind: 'ready',
			value: {
				policy_revision: '7',
				policy: { default_days: 30, maximum_days: 365, allow_never: true },
				items: [resource],
				next: null
			}
		}),
		create: vi.fn().mockResolvedValue(undefined),
		cancel: vi.fn(),
		pending: false
	};
	render(Harness, { language, properties });
	await fireEvent.input(await screen.findByLabelText('ES keys.name'), {
		target: { value: 'Informe 🦀' }
	});
	await fireEvent.click(screen.getByLabelText('ES keys.includeName Private Reports'));
	language.select('en');
	await fireEvent.click(await screen.findByRole('button', { name: 'keys.create' }));
	expect(properties.create).toHaveBeenCalledExactlyOnceWith({
		name: 'Informe 🦀',
		application_id: 'app-id',
		policy_revision: '7',
		expiration: { kind: 'default' },
		grants: [{ resource_id: 'resource-id', selection: { kind: 'all' } }]
	});
	expect(properties.read).toHaveBeenCalledOnce();
});
