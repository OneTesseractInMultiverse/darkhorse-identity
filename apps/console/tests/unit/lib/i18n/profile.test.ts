import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { it, expect, vi, beforeEach } from 'vitest';
import { createLocalization } from '../../../../src/lib/i18n/state';
import { createFormatter } from '../../../../src/lib/i18n/format';
import { contract } from '../../../../src/lib/i18n/contract';
import Harness from './ProfileHarness.svelte';
const profile = {
	id: '00000000-0000-0000-0000-000000000001',
	revision: '0',
	preferred_locale: 'es',
	email: 'ana@example.com',
	active: true,
	email_verified: false,
	first_name: 'Ana',
	second_name: '',
	last_name: 'Guzmán',
	second_last_name: '',
	country: 'CR',
	calling_code: '',
	national_number: '',
	bio: 'Unchanged'
};
function language() {
	const en = Object.entries(contract).map(([key, args]) => [
		key,
		[key, ...Object.keys(args).map((name) => `{${name}}`)].join(' ')
	]);
	const es = en.map(([key, text]) => [key, `ES ${text}`]);
	return createLocalization(createFormatter(contract, { en, es }));
}
beforeEach(() => {
	HTMLDialogElement.prototype.showModal = function () {
		this.open = true;
	};
	HTMLDialogElement.prototype.close = function () {
		this.open = false;
		this.dispatchEvent(new Event('close'));
	};
});
it('preserves focused Unicode input and a pending revision-bound mutation while changing language', async () => {
	const localization = language();
	let finish!: (value: { kind: 'uncertain' }) => void;
	const api = {
		load: vi.fn().mockResolvedValue({ kind: 'ready', value: profile }),
		options: vi.fn().mockResolvedValue({
			kind: 'ready',
			value: {
				country_version: 'fixture',
				phone_version: 'fixture',
				countries: [
					{ code: 'CR', name: 'Costa Rica' },
					{ code: 'DE', name: 'Germany' }
				],
				calling_codes: ['506']
			}
		}),
		save: vi.fn().mockImplementation(
			() =>
				new Promise((resolve) => {
					finish = resolve;
				})
		),
		language: vi.fn()
	};
	render(Harness, { language: localization, api });
	await fireEvent.click(await screen.findByRole('button', { name: 'ES profile.edit' }));
	const firstName = await screen.findByLabelText('ES profile.firstName');
	firstName.focus();
	await fireEvent.input(firstName, { target: { value: 'María 🦀' } });
	expect(screen.getByRole('option', { name: 'Alemania' })).toHaveValue('DE');
	localization.select('en');
	await waitFor(() => expect(screen.getByLabelText('profile.firstName')).toBe(firstName));
	expect(firstName).toHaveFocus();
	expect(firstName).toHaveValue('María 🦀');
	expect(screen.getByRole('option', { name: 'Germany' })).toHaveValue('DE');
	await fireEvent.click(screen.getByRole('button', { name: 'profile.save' }));
	localization.select('es');
	await waitFor(() =>
		expect(screen.getByRole('button', { name: 'ES common.saving' })).toBeDisabled()
	);
	expect(api.save).toHaveBeenCalledExactlyOnceWith(
		'me',
		'0',
		expect.objectContaining({ first_name: 'María 🦀', country: 'CR', bio: 'Unchanged' })
	);
	finish({ kind: 'uncertain' });
	expect(await screen.findByRole('alert')).toHaveTextContent('ES profile.error.uncertain');
	expect(screen.getByRole('button', { name: 'ES profile.edit' })).toBeDisabled();
	expect(api.language).not.toHaveBeenCalled();
});
