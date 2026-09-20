import { render, screen, fireEvent } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import CatalogBindings from '../../../../src/lib/components/admin/CatalogBindings.svelte';
const id = '00000000-0000-0000-0000-000000000001';
function api() {
	return {
		list: vi.fn().mockImplementation((kind) =>
			Promise.resolve({
				kind: 'ready',
				data: {
					items: [{ id, name: kind === 'applications' ? 'Portal' : 'read' }],
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
it('requires an explicit confirmation for application bindings and supports cancelling each step', async () => {
	const save = vi.fn();
	render(CatalogBindings, {
		view: {
			item: { kind: 'capability', id, name: 'read', meaning: 'Read records', active: true },
			applications: [],
			capabilities: [],
			policy_revision: '7'
		},
		api: api(),
		pending: false,
		save
	});
	await fireEvent.click(screen.getByRole('button', { name: 'Bind application' }));
	await screen.findByRole('button', { name: 'Select Portal' });
	await fireEvent.click(screen.getByRole('button', { name: 'Cancel selection' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Bind application' }));
	await fireEvent.click(await screen.findByRole('button', { name: 'Select Portal' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Cancel change' }));
	expect(save).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'Bind application' }));
	await fireEvent.click(await screen.findByRole('button', { name: 'Select Portal' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm change' }));
	expect(save).toHaveBeenCalledWith({
		operation: 'capability_binding',
		application_id: id,
		capability_id: id,
		bound: true
	});
});
it('adds and removes role, resource and scope edges independently', async () => {
	for (const kind of ['role', 'resource', 'scope'] as const) {
		const save = vi.fn();
		const mounted = render(CatalogBindings, {
			view: {
				item: {
					kind,
					id,
					name: 'Target',
					application_id: id,
					resource_id: id,
					audience: 'urn:api'
				},
				applications: [],
				capabilities: [{ kind: 'capability', id, name: 'old', active: false }],
				policy_revision: '7'
			},
			api: api(),
			pending: false,
			save
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Remove old' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Confirm change' }));
		expect(save.mock.lastCall?.[0].operation).toBe(`${kind}_capability`);
		await fireEvent.click(screen.getByRole('button', { name: 'Add capability' }));
		await fireEvent.click(await screen.findByRole('button', { name: 'Select read' }));
		await fireEvent.click(screen.getByRole('button', { name: 'Confirm change' }));
		expect(save).toHaveBeenCalledTimes(2);
		expect(save.mock.lastCall?.[0].capability_id).toBe(id);
		mounted.unmount();
	}
});
