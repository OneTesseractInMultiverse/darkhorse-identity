import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { it, expect, vi, beforeEach } from 'vitest';
import { createLocalization } from '../../../../src/lib/i18n/state';
import { createFormatter } from '../../../../src/lib/i18n/format';
import { contract } from '../../../../src/lib/i18n/contract';
import Harness from './DirectoryHarness.svelte';
const user = {
	id: '00000000-0000-0000-0000-000000000001',
	email: 'ana@example.com',
	first_name: 'Ana',
	last_name: 'Guzmán',
	active: true,
	administrator: false,
	email_verified: false,
	revision: '17'
};
function fixture() {
	const en = Object.entries(contract).map(([key, args]) => [
		key,
		[key, ...Object.keys(args).map((name) => `{${name}}`)].join(' ')
	]);
	const language = createLocalization(
		createFormatter(contract, {
			en,
			es: en.map(([key, value]) => [key, `ES ${value}`])
		})
	);
	language.select('es');
	const api = {
		list: vi
			.fn()
			.mockResolvedValue({ kind: 'ready', data: { actor: user.id, items: [user], next: null } }),
		user: vi.fn().mockResolvedValue({ kind: 'ready', data: user }),
		access: vi.fn().mockResolvedValue({
			kind: 'ready',
			data: {
				user,
				policy_revision: '51',
				applications: [{ id: 'app', name: 'Portal 原文', active: true }],
				selected: 'app',
				roles: [{ id: 'role', name: 'reader:original', assigned: false }]
			}
		}),
		update: vi.fn()
	};
	return { language, api };
}
beforeEach(() => {
	HTMLDialogElement.prototype.showModal = function () {
		this.open = true;
	};
});
it('preserves focused names and revision-bound pending writes when changing language', async () => {
	const { language, api } = fixture();
	let finish!: (result: { kind: 'uncertain' }) => void;
	api.update.mockImplementation(
		() =>
			new Promise((resolve) => {
				finish = resolve;
			})
	);
	render(Harness, { language, api });
	await fireEvent.click(
		await screen.findByRole('button', { name: 'ES directory.viewName Ana Guzmán' })
	);
	await fireEvent.click(await screen.findByRole('button', { name: 'ES directory.editName' }));
	const first = await screen.findByLabelText('ES profile.firstName');
	first.focus();
	await fireEvent.input(first, { target: { value: 'María 🦀' } });
	language.select('en');
	await waitFor(() => expect(screen.getByLabelText('profile.firstName')).toBe(first));
	expect(first).toHaveFocus();
	expect(first).toHaveValue('María 🦀');
	await fireEvent.click(screen.getByRole('button', { name: 'directory.saveName' }));
	language.select('es');
	expect(await screen.findByRole('button', { name: 'ES common.saving' })).toBeDisabled();
	expect(api.update).toHaveBeenCalledExactlyOnceWith(user, {
		kind: 'names',
		first_name: 'María 🦀',
		last_name: 'Guzmán'
	});
	finish({ kind: 'uncertain' });
	expect(await screen.findByRole('alert')).toHaveTextContent('ES directory.error.uncertain');
	language.select('en');
	await waitFor(() =>
		expect(screen.getByRole('alert')).toHaveTextContent('directory.error.uncertain')
	);
	expect(screen.getByRole('button', { name: 'directory.viewName Ana Guzmán' })).toBeDisabled();
	expect(api.list).toHaveBeenCalledOnce();
});
it('preserves literal application and role values without repeating lookups or assignments', async () => {
	const { language, api } = fixture();
	api.update.mockResolvedValue({ kind: 'conflict' });
	render(Harness, { language, api });
	await fireEvent.click(
		await screen.findByRole('button', { name: 'ES directory.assignName Ana Guzmán' })
	);
	await fireEvent.click(await screen.findByLabelText('ES directory.assignRole'));
	language.select('en');
	expect(await screen.findByRole('option', { name: 'Portal 原文' })).toHaveValue('app');
	expect(screen.getByRole('option', { name: 'reader:original' })).toHaveValue('role');
	expect(screen.getByLabelText('directory.assignRole')).toBeChecked();
	await fireEvent.click(screen.getByRole('button', { name: 'directory.saveAccess' }));
	expect(api.update).toHaveBeenCalledExactlyOnceWith(user, {
		kind: 'role',
		application_id: 'app',
		role_id: 'role',
		assigned: true,
		policy_revision: '51'
	});
	expect(await screen.findByRole('alert')).toHaveTextContent('directory.error.conflict');
	expect(api.access).toHaveBeenCalledOnce();
	expect(api.user).toHaveBeenCalledOnce();
});
