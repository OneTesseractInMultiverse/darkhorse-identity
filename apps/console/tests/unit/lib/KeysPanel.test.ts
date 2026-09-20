import { fireEvent, render, screen, waitFor, cleanup } from '@testing-library/svelte';
import { beforeEach, afterEach, it, expect, vi } from 'vitest';
import KeysPanel from '../../../src/lib/components/KeysPanel.svelte';
import { api, key, secret, id } from './key-fixtures';
beforeEach(() => {
	HTMLDialogElement.prototype.showModal = function () {
		this.open = true;
	};
	HTMLDialogElement.prototype.close = function () {
		this.open = false;
		this.dispatchEvent(new Event('close'));
	};
});
afterEach(() => {
	vi.restoreAllMocks();
});
async function open() {
	await waitFor(() => expect(screen.getByRole('button', { name: 'New API key' })).toBeEnabled());
	await fireEvent.click(screen.getByRole('button', { name: 'New API key' }));
	await screen.findByLabelText('Include Invoices');
	await fireEvent.input(screen.getByLabelText('Key name'), { target: { value: 'Build worker' } });
	await fireEvent.click(screen.getByLabelText('Include Invoices'));
}
async function submit() {
	await fireEvent.submit(screen.getByRole('form', { name: 'Create personal API key' }));
}
it('creates an explicit grant, reveals once, and clears the secret on acknowledgement', async () => {
	const service = api();
	render(KeysPanel, { api: service });
	await open();
	await submit();
	expect(await screen.findByLabelText('API key secret')).toHaveValue(secret);
	expect(service.create).toHaveBeenCalledWith({
		name: key.name,
		application_id: id(2),
		policy_revision: '9',
		expiration: { kind: 'default' },
		grants: [{ resource_id: id(3), selection: { kind: 'all' } }]
	});
	await fireEvent.click(screen.getByRole('button', { name: 'I have saved the key' }));
	expect(screen.queryByLabelText('API key secret')).not.toBeInTheDocument();
	expect(document.body.textContent).not.toContain(secret);
	expect(localStorage.getItem('secret')).toBeNull();
	expect(sessionStorage.getItem('secret')).toBeNull();
});
it('clears the single-reveal response when the dialog is dismissed with the keyboard', async () => {
	const service = api();
	render(KeysPanel, { api: service });
	await open();
	await submit();
	await screen.findByLabelText('API key secret');
	(screen.getByRole('dialog') as HTMLDialogElement).close();
	await waitFor(() => expect(screen.queryByLabelText('API key secret')).not.toBeInTheDocument());
	expect(service.list).toHaveBeenCalledTimes(2);
});
it('confirms revocation and provides original grant details separately', async () => {
	const service = api();
	service.list.mockResolvedValueOnce({
		kind: 'ready',
		value: {
			items: [key, { ...key, id: id(6), name: 'Old', active: false, expires_ms: 2000 }],
			next: id(1)
		}
	});
	render(KeysPanel, { api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'View Build worker' }));
	expect(screen.getByText('Resource', { exact: false })).toBeInTheDocument();
	await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
	expect(screen.getByRole('button', { name: 'Revoke Old' })).toBeDisabled();
	await fireEvent.click(screen.getByRole('button', { name: 'Next keys' }));
	await waitFor(() => expect(service.list).toHaveBeenCalledWith(id(1)));
	await fireEvent.click(screen.getByRole('button', { name: 'Revoke Build worker' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
	expect(service.revoke).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'Revoke Build worker' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm revoke key' }));
	expect(await screen.findByText('API key revoked.')).toBeInTheDocument();
	expect(service.revoke).toHaveBeenCalledOnce();
});
it('blocks retries after a lost response, clears private data on sign-out, and reloads explicitly', async () => {
	const service = api();
	service.create.mockResolvedValue({ kind: 'uncertain' });
	render(KeysPanel, { api: service });
	await open();
	await submit();
	expect(await screen.findByRole('alert')).toHaveTextContent('could not be confirmed');
	expect(screen.getByRole('button', { name: 'New API key' })).toBeDisabled();
	expect(service.create).toHaveBeenCalledOnce();
	await fireEvent.click(screen.getByRole('button', { name: 'Refresh keys' }));
	await waitFor(() => expect(screen.getByRole('button', { name: 'New API key' })).toBeEnabled());
	service.revoke.mockResolvedValue({ kind: 'signed-out' });
	await fireEvent.click(screen.getByRole('button', { name: 'Revoke Build worker' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm revoke key' }));
	expect(await screen.findByRole('link', { name: 'Sign in' })).toBeInTheDocument();
	expect(screen.queryByText(key.name)).not.toBeInTheDocument();
});
it('hides secrets on page changes, ignores late delivery after unmount, and denies hidden delivery', async () => {
	const service = api();
	const view = render(KeysPanel, { api: service });
	await open();
	await submit();
	await screen.findByLabelText('API key secret');
	await fireEvent(window, new Event('pagehide'));
	expect(screen.queryByLabelText('API key secret')).not.toBeInTheDocument();
	await fireEvent.click(screen.getByRole('button', { name: 'Refresh keys' }));
	await open();
	vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
	await submit();
	expect(await screen.findByRole('alert')).toHaveTextContent('created while this page was hidden');
	view.unmount();
	vi.restoreAllMocks();
	let finish!: (v: unknown) => void;
	service.create.mockReturnValue(
		new Promise((r) => {
			finish = r;
		})
	);
	const next = render(KeysPanel, { api: service });
	await open();
	await submit();
	next.unmount();
	finish({ kind: 'created', key, secret });
	await Promise.resolve();
	expect(document.body.textContent).not.toContain(secret);
});
it('shows empty, unavailable, and recent authentication states safely', async () => {
	const service = api();
	service.list.mockResolvedValueOnce({ kind: 'ready', value: { items: [], next: null } });
	render(KeysPanel, { api: service });
	expect(await screen.findByText('You have no API keys yet.')).toBeInTheDocument();
	await open();
	service.create.mockResolvedValue({ kind: 'reauthenticate' });
	await submit();
	expect(await screen.findByRole('link', { name: 'Sign in' })).toBeInTheDocument();
	cleanup();
	service.list.mockResolvedValue({ kind: 'unavailable' });
	render(KeysPanel, { api: service });
	expect(await screen.findByRole('alert')).toHaveTextContent('temporarily unavailable');
});
it('does not retain a secret when visibility changes after delivery', async () => {
	const service = api();
	render(KeysPanel, { api: service });
	await open();
	await submit();
	await screen.findByLabelText('API key secret');
	vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
	await fireEvent(document, new Event('visibilitychange'));
	expect(screen.queryByLabelText('API key secret')).not.toBeInTheDocument();
});
