import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { it, expect, vi, beforeEach } from 'vitest';
import { createLocalization } from '../../../../src/lib/i18n/state';
import { createFormatter } from '../../../../src/lib/i18n/format';
import type { Contract } from '../../../../src/lib/i18n/catalog';
import { contract } from '../../../../src/lib/i18n/contract';
import { adminContract } from '../../../../src/lib/i18n/admin-contract';
import Harness from './CatalogHarness.svelte';
const id = '00000000-0000-0000-0000-000000000001';
const client = {
	kind: 'client' as const,
	id,
	name: 'Portal 原文',
	application_id: id,
	active: true,
	revision: '19',
	redirect_uris: ['https://example.test/callback?value=literal'],
	resource_ids: ['resource:literal'],
	scope_ids: ['scope:literal'],
	refresh_tokens: false,
	secrets: []
};
function messages<C extends Contract>(schema: C) {
	const en = Object.entries(schema).map(([key, args]) => [
		key,
		[key, ...Object.keys(args).map((name) => `{${name}}`)].join(' ')
	]);
	return createFormatter(schema, { en, es: en.map(([key, value]) => [key, `ES ${value}`]) });
}
function fixture() {
	const language = createLocalization(messages(contract));
	language.select('es');
	const api = {
		list: vi.fn().mockResolvedValue({
			kind: 'ready',
			data: { items: [client], next: null, policy_revision: '71' }
		}),
		detail: vi.fn().mockResolvedValue({ kind: 'ready', data: client }),
		register: vi.fn(),
		change: vi.fn()
	};
	return { language, format: messages(adminContract), api };
}
beforeEach(() => {
	window.history.replaceState({}, '', `/?application_id=${id}`);
	HTMLDialogElement.prototype.showModal = function () {
		this.open = true;
	};
});
it('retains exact callback, allowances and revision through a language change and uncertain write', async () => {
	const { language, format, api } = fixture();
	let finish!: (value: { kind: 'uncertain' }) => void;
	api.register.mockImplementation(
		() =>
			new Promise((resolve) => {
				finish = resolve;
			})
	);
	render(Harness, { language, format, api, kind: 'clients' });
	await fireEvent.click(await screen.findByRole('button', { name: 'ES viewName Portal 原文' }));
	await fireEvent.click(await screen.findByRole('button', { name: 'ES editClient' }));
	const input = screen.getByLabelText('ES name');
	input.focus();
	await fireEvent.input(input, { target: { value: 'María — service' } });
	language.select('en');
	await waitFor(() => expect(screen.getByLabelText('name')).toBe(input));
	expect(input).toHaveFocus();
	expect(input).toHaveValue('María — service');
	expect(screen.getByLabelText('callbacks')).toHaveValue(client.redirect_uris[0]);
	await fireEvent.submit(input.closest('form')!);
	language.select('es');
	expect(await screen.findByRole('button', { name: 'ES saveChanges' })).toBeDisabled();
	expect(api.register).toHaveBeenCalledExactlyOnceWith({
		operation: 'update_client',
		application_id: id,
		client_id: id,
		revision: '19',
		client: {
			name: 'María — service',
			active: true,
			refresh_tokens: false,
			redirect_uris: client.redirect_uris,
			resource_ids: client.resource_ids,
			scope_ids: client.scope_ids,
			token_endpoint_auth_method: 'client_secret_basic'
		}
	});
	finish({ kind: 'uncertain' });
	expect(await screen.findByRole('alert')).toHaveTextContent('ES error.uncertain');
	expect(screen.getByRole('button', { name: 'ES viewName Portal 原文' })).toBeDisabled();
	expect(api.detail).toHaveBeenCalledOnce();
});
it('keeps one secret reveal across translation and clears it on page hide without a second rotation', async () => {
	const { language, format, api } = fixture();
	api.register.mockResolvedValue({
		kind: 'saved',
		data: { record: client, client_secret: 'a'.repeat(64) }
	});
	render(Harness, { language, format, api, kind: 'clients' });
	await fireEvent.click(await screen.findByRole('button', { name: 'ES viewName Portal 原文' }));
	await fireEvent.click(screen.getByRole('button', { name: 'ES credentials.rotate' }));
	await fireEvent.input(screen.getByLabelText('ES credentials.overlap'), {
		target: { value: '37' }
	});
	language.select('en');
	await fireEvent.click(await screen.findByRole('button', { name: 'credentials.confirmRotation' }));
	expect(await screen.findByLabelText('secret')).toHaveValue('a'.repeat(64));
	language.select('es');
	expect(await screen.findByLabelText('ES secret')).toHaveValue('a'.repeat(64));
	expect(api.register).toHaveBeenCalledExactlyOnceWith({
		operation: 'rotate_secret',
		application_id: id,
		client_id: id,
		revision: '19',
		overlap_seconds: 37
	});
	window.dispatchEvent(new Event('pagehide'));
	await waitFor(() => expect(screen.queryByLabelText('ES secret')).not.toBeInTheDocument());
	expect(screen.getByRole('button', { name: 'ES viewName Portal 原文' })).toBeDisabled();
	expect(localStorage.length).toBe(0);
	expect(sessionStorage.length).toBe(0);
});
it('translates an open binding confirmation while preserving the fresh policy revision and exact target', async () => {
	const { language, format, api } = fixture();
	window.history.replaceState({}, '', '/');
	const role = { kind: 'role', id, name: 'reader:literal' };
	api.list.mockResolvedValue({
		kind: 'ready',
		data: { items: [role], next: null, policy_revision: '70' }
	});
	api.detail.mockResolvedValue({
		kind: 'ready',
		data: {
			item: role,
			policy_revision: '71',
			applications: [{ kind: 'application', id, name: 'Portal 原文' }],
			capabilities: []
		}
	});
	api.change.mockResolvedValue({ kind: 'conflict' });
	render(Harness, { language, format, api, kind: 'roles' });
	await fireEvent.click(await screen.findByRole('button', { name: 'ES viewName reader:literal' }));
	await fireEvent.click(screen.getByRole('button', { name: 'ES binding.unbindName Portal 原文' }));
	expect(screen.getByText('ES binding.removeApplication Portal 原文')).toBeInTheDocument();
	language.select('en');
	expect(await screen.findByText('binding.removeApplication Portal 原文')).toBeInTheDocument();
	expect(api.change).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'binding.confirm' }));
	expect(api.change).toHaveBeenCalledExactlyOnceWith('71', {
		operation: 'role_binding',
		application_id: id,
		role_id: id,
		bound: false
	});
	expect(await screen.findByRole('alert')).toHaveTextContent('error.conflict');
});
