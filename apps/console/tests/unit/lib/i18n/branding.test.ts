import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { it, expect, vi } from 'vitest';
import { createLocalization } from '../../../../src/lib/i18n/state';
import { createFormatter } from '../../../../src/lib/i18n/format';
import { contract } from '../../../../src/lib/i18n/contract';
import Harness from './BrandingHarness.svelte';
it('translates console navigation and branding without repeating reads or changing the target', async () => {
	HTMLDialogElement.prototype.showModal = function () {
		this.open = true;
	};
	const en = Object.entries(contract).map(([key, args]) => [
		key,
		[key, ...Object.keys(args).map((name) => `{${name}}`)].join(' ')
	]);
	const language = createLocalization(
		createFormatter(contract, { en, es: en.map(([key, value]) => [key, `ES ${value}`]) })
	);
	language.select('es');
	const api = {
		settings: vi.fn().mockResolvedValue({
			kind: 'ready',
			value: {
				revision: '7',
				logo: true,
				background: false,
				storage_enabled: true,
				bucket: 'bucket-unchanged'
			}
		}),
		upload: vi.fn(),
		remove: vi.fn().mockResolvedValue({ kind: 'failed', code: 'uncertain' })
	};
	render(Harness, { language, api });
	expect(screen.getByRole('link', { name: /ES console.applications/ })).toHaveAttribute(
		'href',
		'/console/applications'
	);
	await fireEvent.click(await screen.findByRole('button', { name: 'ES branding.changeLogo' }));
	language.select('en');
	expect(
		await screen.findByRole('dialog', { name: 'branding.changeLogoTitle' })
	).toBeInTheDocument();
	await fireEvent.click(screen.getByRole('button', { name: 'media.remove' }));
	await waitFor(() =>
		expect(api.remove).toHaveBeenCalledExactlyOnceWith('/api/admin/branding/logo', '7')
	);
	expect(api.settings).toHaveBeenCalledOnce();
});
