import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { it, expect, vi } from 'vitest';
import KeyEditor from '../../../src/lib/components/KeyEditor.svelte';
import { api, options, id } from './key-fixtures';
it('supports duration, no expiration, exact capability subsets, and one application', async () => {
	const service = api();
	const create = vi.fn();
	const cancel = vi.fn();
	service.options.mockResolvedValue({
		kind: 'ready',
		value: {
			...options,
			items: [
				...options.items,
				{
					...options.items[0],
					application_id: id(9),
					resource_id: id(8),
					resource_name: 'Other API'
				}
			]
		}
	});
	render(KeyEditor, { read: service.options, create, cancel, pending: false });
	await fireEvent.click(await screen.findByLabelText('Include Invoices'));
	expect(screen.getByLabelText('Include Other API')).toBeDisabled();
	await fireEvent.input(screen.getByLabelText('Key name'), { target: { value: ' Worker ' } });
	await fireEvent.change(screen.getByLabelText('Expiration'), { target: { value: 'days' } });
	await fireEvent.input(screen.getByLabelText('Days until expiration'), { target: { value: 7 } });
	await fireEvent.click(screen.getByLabelText('All current permissions'));
	expect(screen.getByRole('button', { name: 'Create key' })).toBeDisabled();
	await fireEvent.click(screen.getByLabelText(/invoices.read/));
	await fireEvent.click(screen.getByLabelText(/invoices.write/));
	await fireEvent.click(screen.getByLabelText(/invoices.write/));
	await fireEvent.submit(screen.getByRole('form'));
	expect(create).toHaveBeenLastCalledWith(
		expect.objectContaining({
			expiration: { kind: 'days', days: 7 },
			grants: [{ resource_id: id(3), selection: { kind: 'subset', capabilities: [id(4)] } }]
		})
	);
	await fireEvent.change(screen.getByLabelText('Expiration'), { target: { value: 'never' } });
	await fireEvent.submit(screen.getByRole('form'));
	expect(create).toHaveBeenLastCalledWith(
		expect.objectContaining({ expiration: { kind: 'never' } })
	);
	await fireEvent.click(screen.getByRole('button', { name: 'Remove Invoices' }));
	expect(screen.getByLabelText('Include Other API')).toBeEnabled();
	await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
	expect(cancel).toHaveBeenCalledOnce();
});
it('keeps selection across pages only for a coherent revision and handles failed permission reads', async () => {
	const service = api();
	const create = vi.fn();
	service.options
		.mockResolvedValueOnce({ kind: 'ready', value: { ...options, next: id(3) } })
		.mockResolvedValueOnce({ kind: 'ready', value: { ...options, policy_revision: '10' } });
	render(KeyEditor, { read: service.options, create, cancel: vi.fn(), pending: false });
	await fireEvent.click(await screen.findByLabelText('Include Invoices'));
	await fireEvent.click(screen.getByRole('button', { name: 'Next resources' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('Permissions changed');
	expect(screen.getByRole('button', { name: 'Create key' })).toBeDisabled();
	service.options.mockResolvedValue({ kind: 'ready', value: { ...options, items: [] } });
	await fireEvent.click(screen.getByRole('button', { name: 'Refresh permissions' }));
	expect(await screen.findByText(/no delegable resource access/)).toBeInTheDocument();
	service.options.mockResolvedValue({ kind: 'unavailable' });
	await fireEvent.click(screen.getByRole('button', { name: 'Refresh permissions' }));
	await waitFor(() =>
		expect(screen.getByRole('alert')).toHaveTextContent('temporarily unavailable')
	);
	await fireEvent.submit(screen.getByRole('form'));
	expect(create).not.toHaveBeenCalled();
});
