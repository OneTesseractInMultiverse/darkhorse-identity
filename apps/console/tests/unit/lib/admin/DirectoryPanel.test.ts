import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { expect, it, vi, beforeEach } from 'vitest';
import DirectoryPanel from '../../../../src/lib/components/admin/DirectoryPanel.svelte';
const id = '00000000-0000-0000-0000-000000000001';
const record = {
	id,
	email: 'ada@example.com',
	first_name: 'Ada',
	last_name: 'Lovelace',
	active: true,
	administrator: true,
	email_verified: false,
	revision: '1'
};
function api() {
	return {
		list: vi
			.fn()
			.mockResolvedValue({ kind: 'ready', data: { actor: id, items: [record], next: null } }),
		user: vi.fn().mockResolvedValue({ kind: 'ready', data: record }),
		access: vi.fn().mockResolvedValue({
			kind: 'ready',
			data: { user: record, applications: [], roles: [], selected: null, policy_revision: '1' }
		}),
		update: vi.fn().mockResolvedValue({ kind: 'saved', user: { ...record, active: false } })
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
it('loads the directory and opens a focused profile without mixing editing into the table', async () => {
	const service = api();
	render(DirectoryPanel, { api: service });
	expect(await screen.findByRole('table')).toBeInTheDocument();
	await fireEvent.click(screen.getByRole('button', { name: 'View Ada Lovelace' }));
	expect(await screen.findByRole('dialog')).toHaveTextContent('ada@example.com');
	expect(screen.queryByLabelText('First name')).not.toBeInTheDocument();
	await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
	await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
});
it('requires confirmation and blocks further mutations after an uncertain response until refresh', async () => {
	const service = api();
	service.update.mockResolvedValue({ kind: 'uncertain' });
	render(DirectoryPanel, { api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'Deactivate Ada Lovelace' }));
	expect(service.update).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
	expect(service.update).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'Deactivate Ada Lovelace' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm deactivation' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('could not be confirmed');
	expect(screen.getByRole('button', { name: 'Deactivate Ada Lovelace' })).toBeDisabled();
	await fireEvent.click(screen.getByRole('button', { name: 'Refresh directory' }));
	await waitFor(() =>
		expect(screen.getByRole('button', { name: 'Deactivate Ada Lovelace' })).toBeEnabled()
	);
	expect(service.update).toHaveBeenCalledTimes(1);
});
it('clears private data when authority is lost and never offers actions on unavailable data', async () => {
	const service = api();
	service.list
		.mockResolvedValueOnce({ kind: 'ready', data: { actor: id, items: [record], next: null } })
		.mockResolvedValue({ kind: 'forbidden' });
	render(DirectoryPanel, { api: service });
	await screen.findByRole('table');
	await fireEvent.click(screen.getByRole('button', { name: 'Refresh directory' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('Administrator access');
	expect(screen.queryByRole('table')).not.toBeInTheDocument();
});
it('edits only names using the freshly loaded user revision and restores focus after closing', async () => {
	const service = api();
	service.user.mockResolvedValue({ kind: 'ready', data: { ...record, revision: '9' } });
	render(DirectoryPanel, { api: service });
	const trigger = await screen.findByRole('button', { name: 'View Ada Lovelace' });
	trigger.focus();
	await fireEvent.click(trigger);
	await fireEvent.click(await screen.findByRole('button', { name: 'Edit name' }));
	const first = await screen.findByLabelText('First name');
	await waitFor(() => expect(first).toHaveFocus());
	await fireEvent.input(first, { target: { value: 'Grace' } });
	await fireEvent.input(screen.getByLabelText('Last name'), { target: { value: 'Hopper' } });
	await fireEvent.click(screen.getByRole('button', { name: 'Save name' }));
	await waitFor(() =>
		expect(service.update).toHaveBeenCalledWith(
			{ ...record, revision: '9' },
			{ kind: 'names', first_name: 'Grace', last_name: 'Hopper' }
		)
	);
	await waitFor(() => expect(screen.getByText('Change saved.')).toHaveFocus());
	await screen.findByRole('table');
	await fireEvent.click(screen.getByRole('button', { name: 'View Ada Lovelace' }));
	await fireEvent.click(await screen.findByRole('button', { name: 'Close' }));
	await waitFor(() =>
		expect(screen.getByRole('button', { name: 'View Ada Lovelace' })).toHaveFocus()
	);
});
it('loads application roles separately and submits both fresh revisions', async () => {
	const service = api(),
		app = '00000000-0000-0000-0000-000000000002',
		role = '00000000-0000-0000-0000-000000000003';
	service.access.mockResolvedValue({
		kind: 'ready',
		data: {
			user: { ...record, revision: '10' },
			policy_revision: '50',
			applications: [{ id: app, name: 'Portal', active: true }],
			selected: app,
			roles: [{ id: role, name: 'Reader', assigned: false }]
		}
	});
	render(DirectoryPanel, { api: service });
	await fireEvent.click(
		await screen.findByRole('button', { name: 'Assign access to Ada Lovelace' })
	);
	const checkbox = await screen.findByLabelText('Assign this role');
	await fireEvent.click(checkbox);
	await fireEvent.click(screen.getByRole('button', { name: 'Save access' }));
	await waitFor(() =>
		expect(service.update).toHaveBeenCalledWith(
			{ ...record, revision: '10' },
			{ kind: 'role', application_id: app, role_id: role, assigned: true, policy_revision: '50' }
		)
	);
});
it('shows empty access catalogs without enabling a write', async () => {
	const service = api();
	render(DirectoryPanel, { api: service });
	await fireEvent.click(
		await screen.findByRole('button', { name: 'Assign access to Ada Lovelace' })
	);
	expect(await screen.findByText('No applications have been registered.')).toBeInTheDocument();
	expect(screen.getByRole('button', { name: 'Save access' })).toBeDisabled();
	await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
	expect(service.update).not.toHaveBeenCalled();
});
it('filters and follows keyset pages while resetting pagination when filters change', async () => {
	const service = api();
	service.list.mockResolvedValue({ kind: 'ready', data: { actor: id, items: [record], next: id } });
	render(DirectoryPanel, { api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'Next page' }));
	await waitFor(() =>
		expect(service.list).toHaveBeenLastCalledWith({ search: '', status: '', after: id })
	);
	await screen.findByRole('table');
	await fireEvent.click(screen.getByRole('button', { name: 'Previous page' }));
	await screen.findByRole('table');
	await fireEvent.input(screen.getByLabelText('Search users'), { target: { value: ' Ada ' } });
	await fireEvent.change(screen.getByLabelText('Status'), { target: { value: 'inactive' } });
	await fireEvent.click(screen.getByRole('button', { name: 'Search' }));
	await waitFor(() =>
		expect(service.list).toHaveBeenLastCalledWith({
			search: 'Ada',
			status: 'inactive',
			after: undefined
		})
	);
});
it('does not update after unmounting or retry a pending mutation', async () => {
	const service = api();
	let complete!: (value: unknown) => void;
	service.update.mockImplementation(
		() =>
			new Promise((resolve) => {
				complete = resolve;
			})
	);
	const view = render(DirectoryPanel, { api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'Deactivate Ada Lovelace' }));
	await fireEvent.click(await screen.findByRole('button', { name: 'Confirm deactivation' }));
	expect(screen.getByRole('button', { name: 'Saving…' })).toBeDisabled();
	view.unmount();
	complete({ kind: 'saved', user: record });
	await Promise.resolve();
	expect(service.update).toHaveBeenCalledTimes(1);
	expect(service.list).toHaveBeenCalledTimes(1);
});
it('clears the table if opening a fresh profile loses authority', async () => {
	const service = api();
	service.user.mockResolvedValue({ kind: 'signed-out' });
	render(DirectoryPanel, { api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'View Ada Lovelace' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('Sign in');
	expect(screen.queryByRole('table')).not.toBeInTheDocument();
	expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});
it('prevents Escape while a mutation is pending and permits native dialog closure while idle', async () => {
	const service = api();
	service.update.mockImplementation(() => new Promise(() => {}));
	render(DirectoryPanel, { api: service });
	await fireEvent.click(await screen.findByRole('button', { name: 'Deactivate Ada Lovelace' }));
	let dialog = await screen.findByRole('dialog');
	const idle = new Event('cancel', { cancelable: true });
	dialog.dispatchEvent(idle);
	expect(idle.defaultPrevented).toBe(false);
	await fireEvent(dialog, new Event('close'));
	await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
	await fireEvent.click(screen.getByRole('button', { name: 'Deactivate Ada Lovelace' }));
	await fireEvent.click(await screen.findByRole('button', { name: 'Confirm deactivation' }));
	dialog = screen.getByRole('dialog');
	const pending = new Event('cancel', { cancelable: true });
	dialog.dispatchEvent(pending);
	expect(pending.defaultPrevented).toBe(true);
	await fireEvent(dialog, new Event('close'));
	expect(screen.getByRole('dialog')).toBeInTheDocument();
});
it('recovers an unavailable access lookup and clears private content on a later denial', async () => {
	const service = api();
	service.access
		.mockResolvedValueOnce({ kind: 'unavailable' })
		.mockResolvedValue({ kind: 'forbidden' });
	render(DirectoryPanel, { api: service });
	await fireEvent.click(
		await screen.findByRole('button', { name: 'Assign access to Ada Lovelace' })
	);
	expect(await screen.findByRole('alert')).toHaveTextContent('temporarily unavailable');
	expect(screen.getByRole('button', { name: 'Close' })).toBeEnabled();
	await fireEvent.click(screen.getByRole('button', { name: 'Retry access lookup' }));
	await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
	expect(screen.queryByRole('table')).not.toBeInTheDocument();
});
it('loads a different application and permits removing an existing inactive assignment', async () => {
	const service = api(),
		app = '00000000-0000-0000-0000-000000000002',
		role = '00000000-0000-0000-0000-000000000003';
	const data = {
		user: record,
		policy_revision: '5',
		applications: [{ id: app, name: 'Old application', active: false }],
		selected: app,
		roles: [] as { id: string; name: string; assigned: boolean }[]
	};
	service.access.mockResolvedValueOnce({ kind: 'ready', data }).mockResolvedValue({
		kind: 'ready',
		data: { ...data, roles: [{ id: role, name: 'Reader', assigned: true }] }
	});
	render(DirectoryPanel, { api: service });
	await fireEvent.click(
		await screen.findByRole('button', { name: 'Assign access to Ada Lovelace' })
	);
	expect(
		await screen.findByText('No roles are available for this application.')
	).toBeInTheDocument();
	await fireEvent.change(screen.getByLabelText('Application'), { target: { value: app } });
	await screen.findByLabelText('Role');
	await fireEvent.change(screen.getByLabelText('Role'), { target: { value: role } });
	expect(screen.getByRole('button', { name: 'Save access' })).toBeDisabled();
	await fireEvent.click(screen.getByLabelText('Assign this role'));
	await fireEvent.click(screen.getByRole('button', { name: 'Save access' }));
	await waitFor(() =>
		expect(service.update).toHaveBeenCalledWith(record, {
			kind: 'role',
			application_id: app,
			role_id: role,
			assigned: false,
			policy_revision: '5'
		})
	);
});
