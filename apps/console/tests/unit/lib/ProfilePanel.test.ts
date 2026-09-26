import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/svelte';
import { it, expect, vi, beforeEach, afterEach } from 'vitest';
import ProfilePanel from '../../../src/lib/components/ProfilePanel.svelte';
import type { ProfileApi } from '../../../src/lib/profiles';
const profile = {
	id: '00000000-0000-0000-0000-000000000001',
	revision: '0',
	preferred_locale: null,
	email: 'ana@example.com',
	active: true,
	email_verified: false,
	first_name: 'Ana',
	second_name: '',
	last_name: 'Guzmán',
	second_last_name: '',
	country: 'CR',
	calling_code: '506',
	national_number: '88887777',
	bio: 'Hello'
};
const options = {
	country_version: 'test',
	phone_version: 'test',
	countries: [{ code: 'CR', name: 'Costa Rica' }],
	calling_codes: ['506', '1']
};
function api() {
	return {
		language: vi.fn().mockResolvedValue({
			kind: 'ready',
			value: { ...profile, revision: '1', preferred_locale: 'es' }
		}),
		load: vi.fn().mockResolvedValue({ kind: 'ready', value: profile }),
		options: vi.fn().mockResolvedValue({ kind: 'ready', value: options }),
		save: vi.fn().mockResolvedValue({
			kind: 'ready',
			value: { ...profile, revision: '1', first_name: 'María' }
		})
	};
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
afterEach(() => vi.restoreAllMocks());
async function edit() {
	await fireEvent.click(await screen.findByRole('button', { name: 'Edit profile' }));
	await screen.findByLabelText('First name');
}
it('edits bounded Unicode data in a focused form without security fields', async () => {
	const service = api();
	render(ProfilePanel, { api: service });
	await edit();
	expect(screen.getByRole('option', { name: 'Costa Rica' })).toBeInTheDocument();
	await fireEvent.input(screen.getByLabelText('First name'), { target: { value: 'María' } });
	await fireEvent.input(screen.getByLabelText('Bio (optional)'), {
		target: { value: '🦀'.repeat(2000) }
	});
	await fireEvent.input(screen.getByLabelText('Second name (optional)'), {
		target: { value: 'José' }
	});
	await fireEvent.input(screen.getByLabelText('Last name'), { target: { value: 'de la Cruz' } });
	await fireEvent.input(screen.getByLabelText('Second last name (optional)'), {
		target: { value: 'Guzmán' }
	});
	await fireEvent.change(screen.getByLabelText('Country (optional)'), { target: { value: 'CR' } });
	await fireEvent.change(screen.getByLabelText('Calling code (optional)'), {
		target: { value: '1' }
	});
	await fireEvent.input(screen.getByLabelText('National phone number (optional)'), {
		target: { value: '2025550123' }
	});
	await fireEvent.submit(screen.getByLabelText('First name').closest('form')!);
	await screen.findByText('Profile saved.');
	expect(service.save).toHaveBeenCalledWith(
		'me',
		'0',
		expect.objectContaining({ first_name: 'María', bio: '🦀'.repeat(2000) })
	);
	expect(service.save.mock.calls[0][2]).not.toHaveProperty('email');
	await edit();
	await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
	expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});
it('blocks uncertain writes and clears private details when access is lost', async () => {
	const service = api();
	service.save.mockResolvedValue({ kind: 'uncertain' });
	render(ProfilePanel, { api: service });
	await edit();
	await fireEvent.submit(screen.getByLabelText('First name').closest('form')!);
	await screen.findByRole('alert');
	expect(screen.getByRole('button', { name: 'Edit profile' })).toBeDisabled();
	expect(service.save).toHaveBeenCalledOnce();
	service.load.mockResolvedValue({ kind: 'denied' });
	await fireEvent.click(screen.getByRole('button', { name: 'Reload profile' }));
	await waitFor(() =>
		expect(screen.queryByText('ana@example.com', { exact: false })).not.toBeInTheDocument()
	);
});
it('rejects late reads after leaving and handles unavailable edit options', async () => {
	let resolve!: (v: Awaited<ReturnType<ProfileApi['load']>>) => void;
	const service = api();
	service.load.mockReturnValue(
		new Promise((r) => {
			resolve = r;
		})
	);
	render(ProfilePanel, { api: service });
	cleanup();
	resolve({ kind: 'ready', value: profile });
	await Promise.resolve();
	expect(screen.queryByText('ana@example.com', { exact: false })).not.toBeInTheDocument();
	const second = api();
	second.options.mockResolvedValue({ kind: 'recent' });
	render(ProfilePanel, { props: { api: second, target: profile.id } });
	await fireEvent.click(await screen.findByRole('button', { name: 'Edit profile' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('Sign out and sign in');
	expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});

it('falls back safely when a private image is absent and supports closing either dialog without a write', async () => {
	const service = api(),
		images = { settings: vi.fn(), upload: vi.fn(), remove: vi.fn() };
	render(ProfilePanel, { api: service, images });
	await screen.findByRole('button', { name: 'Change picture' });
	await fireEvent.error(screen.getByAltText('User avatar'));
	expect(screen.getByAltText('User avatar')).not.toHaveAttribute('src', '/api/profiles/me/picture');
	await edit();
	(screen.getByRole('dialog') as HTMLDialogElement).close();
	await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
	await fireEvent.click(screen.getByRole('button', { name: 'Change picture' }));
	(screen.getByRole('dialog') as HTMLDialogElement).close();
	await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
	await fireEvent.click(screen.getByRole('button', { name: 'Change picture' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
	expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
	expect(images.upload).not.toHaveBeenCalled();
	expect(images.remove).not.toHaveBeenCalled();
	expect(service.save).not.toHaveBeenCalled();
});
it('requires an explicit account language save and blocks uncertain mutations until reload', async () => {
	const service = api();
	render(ProfilePanel, { api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'Change language' }));
	await fireEvent.change(screen.getByLabelText('Preferred language'), { target: { value: 'es' } });
	expect(service.language).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'Save language' }));
	expect(await screen.findByRole('status')).toHaveTextContent('Se guardó el idioma preferido.');
	expect(service.language).toHaveBeenCalledWith('0', 'es');
	await fireEvent.click(screen.getByRole('button', { name: 'Cambiar idioma' }));
	service.language.mockResolvedValue({ kind: 'uncertain' });
	await fireEvent.change(screen.getByLabelText('Idioma preferido'), { target: { value: '' } });
	await fireEvent.click(screen.getByRole('button', { name: 'Guardar idioma' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('No se pudo confirmar');
	expect(screen.getByRole('button', { name: 'Cambiar idioma' })).toBeDisabled();
	expect(service.language).toHaveBeenCalledTimes(2);
});
