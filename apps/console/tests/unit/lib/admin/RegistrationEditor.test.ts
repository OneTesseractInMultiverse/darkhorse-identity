import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/svelte';
import { expect, it, vi, afterEach } from 'vitest';
import type { ComponentProps } from 'svelte';
import RegistrationEditor from '../../../../src/lib/components/admin/RegistrationEditor.svelte';
function renderEditor(props: ComponentProps<typeof RegistrationEditor>) {
	return render(RegistrationEditor, { props });
}
const id = '00000000-0000-0000-0000-000000000001';
const second = '00000000-0000-0000-0000-000000000002';
function api() {
	return {
		list: vi.fn().mockImplementation((kind) =>
			Promise.resolve({
				kind: 'ready',
				data: {
					items: [
						{
							id,
							name: kind === 'resources' ? 'API' : 'read',
							kind: kind === 'resources' ? 'resource' : 'scope'
						}
					],
					next: null,
					policy_revision: '7'
				}
			})
		),
		detail: vi.fn(),
		register: vi.fn(),
		change: vi.fn()
	};
}
afterEach(() => vi.unstubAllGlobals());
it('creates an application with an explicitly selected active owner and edits its fresh revision', async () => {
	vi.stubGlobal(
		'fetch',
		vi.fn().mockImplementation(() =>
			Promise.resolve(
				new Response(
					JSON.stringify({
						actor: id,
						items: [
							{
								id,
								email: 'owner@example.com',
								first_name: 'Ada',
								last_name: 'Lovelace',
								active: true,
								administrator: false,
								email_verified: true,
								revision: '0'
							}
						],
						next: null
					}),
					{ status: 200 }
				)
			)
		)
	);
	const save = vi.fn(),
		cancel = vi.fn();
	let mounted = renderEditor({
		kind: 'applications',
		api: api(),
		pending: false,
		save,
		cancel
	});
	await fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'Portal' } });
	expect(screen.getByRole('button', { name: 'Create' })).toBeDisabled();
	await fireEvent.click(await screen.findByRole('button', { name: 'Select owner@example.com' }));
	await fireEvent.submit(screen.getByLabelText('Name').closest('form')!);
	expect(save).toHaveBeenCalledWith({
		operation: 'create_application',
		application: { name: 'Portal', owner_id: id, active: true }
	});
	await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
	expect(cancel).toHaveBeenCalledOnce();
	mounted.unmount();
	mounted = renderEditor({
		kind: 'applications',
		target: {
			kind: 'application',
			id,
			name: 'Portal',
			active: true,
			owner_id: id,
			owner_email: 'owner@example.com',
			revision: '9007199254740993'
		},
		api: api(),
		pending: false,
		save,
		cancel
	});
	await fireEvent.click(screen.getByLabelText('Active'));
	await fireEvent.submit(screen.getByLabelText('Name').closest('form')!);
	expect(save).toHaveBeenLastCalledWith({
		operation: 'update_application',
		application_id: id,
		revision: '9007199254740993',
		application: { name: 'Portal', owner_id: id, active: false }
	});
	mounted.unmount();
});
it('creates immutable resources, resource-scoped scopes and capability meanings', async () => {
	for (const kind of ['resources', 'scopes', 'capabilities'] as const) {
		const save = vi.fn();
		const mounted = renderEditor({
			kind,
			application: id,
			api: api(),
			pending: false,
			save,
			cancel: vi.fn()
		});
		await fireEvent.input(
			screen.getByLabelText(kind === 'capabilities' ? 'Permission key' : 'Name'),
			{ target: { value: 'records.read' } }
		);
		if (kind === 'capabilities')
			await fireEvent.input(screen.getByLabelText('Meaning'), {
				target: { value: ' Read records ' }
			});
		if (kind === 'scopes')
			await fireEvent.click(await screen.findByRole('button', { name: 'Select API' }));
		await fireEvent.submit(
			screen.getByLabelText(kind === 'capabilities' ? 'Permission key' : 'Name').closest('form')!
		);
		expect(save).toHaveBeenCalledWith(
			kind === 'resources'
				? { operation: 'create_resource', application_id: id, name: 'records.read' }
				: kind === 'scopes'
					? { operation: 'create_scope', application_id: id, resource_id: id, name: 'records.read' }
					: {
							operation: 'create_capability',
							application_id: id,
							key: 'records.read',
							meaning: 'Read records'
						}
		);
		mounted.unmount();
	}
});
it('edits exact callbacks and explicit client allowances without duplicate additions', async () => {
	const save = vi.fn(),
		service = api();
	renderEditor({
		kind: 'clients',
		application: id,
		target: {
			kind: 'client',
			id: second,
			name: 'Web',
			active: true,
			revision: '9',
			redirect_uris: ['https://app.example/cb'],
			resource_ids: [second],
			scope_ids: [second],
			refresh_tokens: false
		},
		api: service,
		pending: false,
		save,
		cancel: vi.fn()
	});
	await fireEvent.click(await screen.findByRole('button', { name: 'Select API' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Select API' }));
	await fireEvent.click(await screen.findByRole('button', { name: 'Select read' }));
	await fireEvent.click(screen.getByRole('button', { name: `Remove resource ${second}` }));
	await fireEvent.click(screen.getByRole('button', { name: `Remove scope ${second}` }));
	await fireEvent.input(screen.getByLabelText('Callback URLs'), {
		target: { value: 'https://app.example/new\n\nhttps://app.example/second' }
	});
	await fireEvent.click(screen.getByLabelText('Allow refresh tokens'));
	await fireEvent.submit(screen.getByLabelText('Name').closest('form')!);
	expect(save).toHaveBeenCalledWith({
		operation: 'update_client',
		application_id: id,
		client_id: second,
		revision: '9',
		client: {
			name: 'Web',
			active: true,
			refresh_tokens: true,
			redirect_uris: ['https://app.example/new', 'https://app.example/second'],
			resource_ids: [id],
			scope_ids: [id],
			token_endpoint_auth_method: 'client_secret_basic'
		}
	});
	cleanup();
	renderEditor({
		kind: 'clients',
		application: id,
		api: api(),
		pending: false,
		save,
		cancel: vi.fn()
	});
	await fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'New' } });
	await fireEvent.input(screen.getByLabelText('Callback URLs'), {
		target: { value: 'https://app.example/' }
	});
	await waitFor(() => expect(screen.getByRole('button', { name: 'Select API' })).toBeEnabled());
	await fireEvent.submit(screen.getByLabelText('Name').closest('form')!);
	expect(save.mock.lastCall?.[0].operation).toBe('create_client');
});
