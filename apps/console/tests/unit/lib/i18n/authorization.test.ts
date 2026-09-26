import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { it, expect, vi } from 'vitest';
import { createLocalization } from '../../../../src/lib/i18n/state';
import { createFormatter } from '../../../../src/lib/i18n/format';
import { contract } from '../../../../src/lib/i18n/contract';
import Harness from './AuthorizationHarness.svelte';
it('translates consent without changing untrusted application text, scope values or the explicit decision', async () => {
	const en = Object.entries(contract).map(([key, args]) => [
		key,
		[key, ...Object.keys(args).map((name) => `{${name}}`)].join(' ')
	]);
	const language = createLocalization(
		createFormatter(contract, { en, es: en.map(([key, text]) => [key, `ES ${text}`]) })
	);
	language.select('es');
	const pending = {
		kind: 'pending',
		request_id: 'a'.repeat(64),
		client_name: '<img src=x>Reports',
		scopes: ['openid', 'reports:read'],
		resource: 'https://api.example/reports',
		status: 'consent'
	} as const;
	const properties = {
		load: vi.fn().mockResolvedValue(pending),
		decide: vi
			.fn()
			.mockResolvedValue({ kind: 'redirect', url: 'https://app.example/callback?code=unchanged' }),
		signIn: vi.fn(),
		navigate: vi.fn()
	};
	const view = render(Harness, { language, properties });
	expect(
		await screen.findByRole('heading', { name: 'ES authorization.connect <img src=x>Reports' })
	).toBeInTheDocument();
	expect(view.container.querySelector('img')).toBeNull();
	expect(screen.getByText('reports:read')).toBeInTheDocument();
	expect(properties.decide).not.toHaveBeenCalled();
	language.select('en');
	await fireEvent.click(await screen.findByRole('button', { name: 'authorization.allow' }));
	await waitFor(() =>
		expect(properties.decide).toHaveBeenCalledExactlyOnceWith('a'.repeat(64), 'approve')
	);
	expect(properties.navigate).toHaveBeenCalledExactlyOnceWith(
		'https://app.example/callback?code=unchanged'
	);
});
