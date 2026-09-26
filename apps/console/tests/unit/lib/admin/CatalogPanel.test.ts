import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { beforeEach, afterEach, it, expect, vi } from 'vitest';
import CatalogPanel from '../../../../src/lib/components/admin/CatalogPanel.svelte';
afterEach(() => vi.unstubAllGlobals());
const id = '00000000-0000-0000-0000-000000000001';
const role = { kind: 'role' as const, id, name: 'Reader' };
const client = {
	kind: 'client' as const,
	id,
	name: 'Web',
	application_id: id,
	active: true,
	revision: '9',
	redirect_uris: ['https://web.example/cb'],
	resource_ids: [],
	scope_ids: [],
	refresh_tokens: false,
	secrets: []
};
function api(record = role as typeof role | typeof client) {
	return {
		list: vi.fn().mockResolvedValue({
			kind: 'ready',
			data: { items: [record], next: null, policy_revision: '7' }
		}),
		detail: vi.fn().mockResolvedValue({
			kind: 'ready',
			data: { item: record, applications: [], capabilities: [], policy_revision: '8' }
		}),
		change: vi.fn().mockResolvedValue({ kind: 'saved', data: { policy_revision: '9' } }),
		register: vi
			.fn()
			.mockResolvedValue({ kind: 'saved', data: { record: client, client_secret: 'a'.repeat(64) } })
	};
}
beforeEach(() => {
	window.history.replaceState({}, '', '/');
	HTMLDialogElement.prototype.showModal = function () {
		this.open = true;
	};
	HTMLDialogElement.prototype.close = function () {
		this.open = false;
		this.dispatchEvent(new Event('close'));
	};
});
it('creates a shared role with the displayed revision and no implicit application', async () => {
	const service = api();
	render(CatalogPanel, { kind: 'roles', api: service });
	await waitFor(() => expect(screen.getByRole('button', { name: 'Create role' })).toBeEnabled());
	await fireEvent.click(screen.getByRole('button', { name: 'Create role' }));
	await fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'Writer' } });
	await fireEvent.submit(screen.getByLabelText('Name').closest('form')!);
	await waitFor(() =>
		expect(service.change).toHaveBeenCalledWith('7', {
			operation: 'create_role',
			name: 'Writer',
			application_id: undefined
		})
	);
	expect(await screen.findByText('Access policy saved.')).toBeInTheDocument();
});
it('confirms graph writes with the fresh detail revision and blocks after uncertainty', async () => {
	const service = api();
	service.change.mockResolvedValue({ kind: 'uncertain' });
	service.detail.mockResolvedValue({
		kind: 'ready',
		data: {
			item: role,
			applications: [{ kind: 'application', id, name: 'Portal' }],
			capabilities: [],
			policy_revision: '8'
		}
	});
	render(CatalogPanel, { kind: 'roles', api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'View Reader' }));
	await fireEvent.click(await screen.findByRole('button', { name: 'Unbind Portal' }));
	expect(service.change).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm change' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('could not be confirmed');
	expect(service.change).toHaveBeenCalledWith('8', {
		operation: 'role_binding',
		application_id: id,
		role_id: id,
		bound: false
	});
	expect(screen.getByRole('button', { name: 'View Reader' })).toBeDisabled();
	await fireEvent.click(screen.getByRole('button', { name: 'Refresh catalog' }));
	await waitFor(() => expect(screen.getByRole('button', { name: 'View Reader' })).toBeEnabled());
	expect(service.change).toHaveBeenCalledTimes(1);
});
it('reveals a rotated secret only until acknowledgment and clears it on page hide', async () => {
	window.history.replaceState({}, '', `/?application_id=${id}`);
	const service = api(client);
	service.detail.mockResolvedValue({ kind: 'ready', data: client });
	render(CatalogPanel, { kind: 'clients', api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'View Web' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Rotate secret' }));
	expect(service.register).not.toHaveBeenCalled();
	await fireEvent.submit(
		screen.getByLabelText('Previous secret overlap (seconds)').closest('form')!
	);
	expect(await screen.findByLabelText('Client secret')).toHaveValue('a'.repeat(64));
	expect(service.register).toHaveBeenCalledWith({
		operation: 'rotate_secret',
		application_id: id,
		client_id: id,
		revision: '9',
		overlap_seconds: 0
	});
	window.dispatchEvent(new Event('pagehide'));
	await waitFor(() => expect(screen.queryByLabelText('Client secret')).not.toBeInTheDocument());
	expect(screen.getByRole('button', { name: 'View Web' })).toBeDisabled();
	expect(localStorage.length).toBe(0);
	expect(sessionStorage.length).toBe(0);
});
it('clears records when authority is lost and requires application context for child catalogs', async () => {
	const service = api();
	service.list.mockResolvedValue({ kind: 'forbidden' });
	const rendered = render(CatalogPanel, { kind: 'roles', api: service });
	expect(await screen.findByRole('alert')).toHaveTextContent('Administrator access');
	expect(screen.queryByRole('table')).not.toBeInTheDocument();
	rendered.unmount();
	const other = api();
	render(CatalogPanel, { kind: 'clients', api: other });
	await screen.findByRole('group', { name: 'Applications' });
	expect(other.list).toHaveBeenCalledWith('applications', { search: '', after: undefined });
	expect(other.list).not.toHaveBeenCalledWith('clients', expect.anything());
});
it('searches and pages live catalogs, resets filters and selects explicit application context', async () => {
	const service = api();
	service.list.mockResolvedValue({
		kind: 'ready',
		data: { items: [role], next: '00000000-0000-0000-0000-000000000002', policy_revision: '7' }
	});
	let rendered = render(CatalogPanel, { kind: 'roles', api: service });
	await screen.findByRole('table');
	await fireEvent.click(screen.getByRole('button', { name: 'Next' }));
	await waitFor(() => expect(screen.getByText('Page 2')).toBeInTheDocument());
	await fireEvent.click(screen.getByRole('button', { name: 'Previous' }));
	await fireEvent.input(screen.getByLabelText('Search roles'), { target: { value: ' Reader ' } });
	await fireEvent.submit(screen.getByRole('form', { name: 'Search catalog' }));
	await waitFor(() =>
		expect(service.list).toHaveBeenLastCalledWith('roles', {
			search: ' Reader ',
			status: '',
			after: undefined,
			application_id: undefined
		})
	);
	rendered.unmount();
	const other = api();
	other.list.mockImplementation((kind) =>
		Promise.resolve({
			kind: 'ready',
			data: {
				items: kind === 'applications' ? [{ kind: 'application', id, name: 'Portal' }] : [],
				next: null,
				policy_revision: '7'
			}
		})
	);
	rendered = render(CatalogPanel, { kind: 'clients', api: other });
	await fireEvent.click(await screen.findByRole('button', { name: 'Select Portal' }));
	await screen.findByRole('table');
	expect(other.list).toHaveBeenLastCalledWith('clients', { application_id: id, after: undefined });
	await fireEvent.click(screen.getByRole('button', { name: 'Choose application' }));
	await screen.findByRole('group', { name: 'Applications' });
	rendered.unmount();
	window.history.replaceState({}, '', `/?application_id=${id}`);
	const shared = api();
	render(CatalogPanel, { kind: 'roles', api: shared });
	await screen.findByRole('table');
	await fireEvent.click(screen.getByRole('button', { name: 'Show shared catalog' }));
	await waitFor(() =>
		expect(shared.list).toHaveBeenLastCalledWith('roles', {
			after: undefined,
			application_id: undefined
		})
	);
});
it('edits application records, clears denied details, and rejects malformed route context', async () => {
	vi.stubGlobal(
		'fetch',
		vi
			.fn()
			.mockImplementation(() =>
				Promise.resolve(
					new Response(JSON.stringify({ actor: id, items: [], next: null }), { status: 200 })
				)
			)
	);
	const record = {
		kind: 'application' as const,
		id,
		name: 'Portal',
		active: true,
		revision: '4',
		owner_id: id,
		owner_email: 'owner@example.com'
	};
	const service = api();
	service.list.mockResolvedValue({
		kind: 'ready',
		data: { items: [record], next: null, policy_revision: '7' }
	});
	service.detail.mockResolvedValue({ kind: 'ready', data: record });
	service.register.mockResolvedValue({ kind: 'saved', data: { record } });
	const rendered = render(CatalogPanel, { kind: 'applications', api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'View Portal' }));
	expect(await screen.findByRole('link', { name: 'Clients' })).toHaveAttribute(
		'href',
		`/console/clients?application_id=${id}`
	);
	await fireEvent.click(screen.getByRole('button', { name: 'Edit application' }));
	await fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'Edited' } });
	await fireEvent.submit(screen.getByLabelText('Name').closest('form')!);
	await screen.findByText('Change saved.');
	expect(service.register.mock.lastCall?.[0].revision).toBe('4');
	service.detail.mockResolvedValue({ kind: 'signed-out' });
	await fireEvent.click(await screen.findByRole('button', { name: 'View Portal' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('Sign in');
	expect(screen.queryByRole('table')).not.toBeInTheDocument();
	rendered.unmount();
	window.history.replaceState({}, '', '/?application_id=bad');
	const invalid = api();
	render(CatalogPanel, { kind: 'clients', api: invalid });
	expect(await screen.findByRole('alert')).toHaveTextContent('Invalid application');
	expect(invalid.list).not.toHaveBeenCalled();
});
it('confirms permanent capability retirement and supports cancelling the dialog', async () => {
	const cap = {
		kind: 'capability' as const,
		id,
		name: 'records.read',
		active: true,
		meaning: 'Read records'
	};
	const service = api();
	service.list.mockResolvedValue({
		kind: 'ready',
		data: { items: [cap], next: null, policy_revision: '7' }
	});
	service.detail.mockResolvedValue({
		kind: 'ready',
		data: { item: cap, applications: [], capabilities: [], policy_revision: '8' }
	});
	render(CatalogPanel, { kind: 'capabilities', api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'View records.read' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Retire capability' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Cancel retirement' }));
	expect(service.change).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
	await fireEvent.click(screen.getByRole('button', { name: 'View records.read' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Retire capability' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm capability retirement' }));
	await waitFor(() =>
		expect(service.change).toHaveBeenCalledWith('8', {
			operation: 'retire_capability',
			capability_id: id
		})
	);
});
it('acknowledges secrets, handles rejected credential changes, and edits clients in a separate form', async () => {
	window.history.replaceState({}, '', `/?application_id=${id}`);
	const service = api(client);
	service.detail.mockResolvedValue({ kind: 'ready', data: client });
	render(CatalogPanel, { kind: 'clients', api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'View Web' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Edit client' }));
	await fireEvent.submit(screen.getByLabelText('Name').closest('form')!);
	await screen.findByLabelText('Client secret');
	await fireEvent.click(screen.getByRole('button', { name: 'I have stored the secret' }));
	await waitFor(() => expect(screen.queryByLabelText('Client secret')).not.toBeInTheDocument());
	service.register.mockResolvedValue({ kind: 'recent' });
	await fireEvent.click(await screen.findByRole('button', { name: 'View Web' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Rotate secret' }));
	await fireEvent.submit(screen.getByRole('button', { name: 'Confirm rotation' }).closest('form')!);
	expect(await screen.findByRole('alert')).toHaveTextContent('sign out and sign in');
	expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});
it('discards a secret response that arrives after the page becomes hidden', async () => {
	window.history.replaceState({}, '', `/?application_id=${id}`);
	const service = api(client);
	service.detail.mockResolvedValue({ kind: 'ready', data: client });
	let finish!: (v: unknown) => void;
	service.register.mockImplementation(
		() =>
			new Promise((resolve) => {
				finish = resolve;
			})
	);
	render(CatalogPanel, { kind: 'clients', api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'View Web' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Rotate secret' }));
	await fireEvent.submit(screen.getByRole('button', { name: 'Confirm rotation' }).closest('form')!);
	const visibility = vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
	document.dispatchEvent(new Event('visibilitychange'));
	finish({ kind: 'saved', data: { record: client, client_secret: 'a'.repeat(64) } });
	await waitFor(() =>
		expect(screen.getByText(/Secret dismissed while the page was hidden/)).toBeInTheDocument()
	);
	expect(screen.queryByLabelText('Client secret')).not.toBeInTheDocument();
	visibility.mockRestore();
});
it('sends explicit status filters instead of filtering a partial page locally', async () => {
	const service = api();
	render(CatalogPanel, { kind: 'applications', api: service });
	await screen.findByRole('table');
	await fireEvent.change(screen.getByLabelText('Status'), { target: { value: 'inactive' } });
	await fireEvent.submit(screen.getByRole('form', { name: 'Search catalog' }));
	await waitFor(() =>
		expect(service.list).toHaveBeenLastCalledWith('applications', {
			search: '',
			status: 'inactive',
			after: undefined,
			application_id: undefined
		})
	);
});
it('clears the previous success message while a subsequent write is pending', async () => {
	const cap = { kind: 'capability' as const, id, name: 'read', active: true, meaning: 'Read' };
	const service = api();
	service.list.mockResolvedValue({
		kind: 'ready',
		data: { items: [cap], next: null, policy_revision: '7' }
	});
	service.detail.mockResolvedValue({
		kind: 'ready',
		data: { item: cap, applications: [], capabilities: [], policy_revision: '8' }
	});
	render(CatalogPanel, { kind: 'capabilities', api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'View read' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Retire capability' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm capability retirement' }));
	await screen.findByText('Access policy saved.');
	await screen.findByRole('table');
	let finish!: (v: unknown) => void;
	service.change.mockImplementation(
		() =>
			new Promise((resolve) => {
				finish = resolve;
			})
	);
	await fireEvent.click(screen.getByRole('button', { name: 'View read' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Retire capability' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm capability retirement' }));
	expect(screen.queryByText('Access policy saved.')).not.toBeInTheDocument();
	finish({ kind: 'conflict' });
	expect(await screen.findByRole('alert')).toHaveTextContent('changed');
});
