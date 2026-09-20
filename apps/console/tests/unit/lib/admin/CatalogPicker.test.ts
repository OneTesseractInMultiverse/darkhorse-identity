import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import CatalogPicker from '../../../../src/lib/components/admin/CatalogPicker.svelte';
it('searches and pages options, reports errors and only selects explicit choices', async () => {
	const load = vi
		.fn()
		.mockResolvedValueOnce({
			kind: 'ready',
			data: { items: [{ id: 'one', name: 'Portal' }], next: 'two' }
		})
		.mockResolvedValueOnce({
			kind: 'ready',
			data: { items: [{ id: 'two', name: 'Other' }], next: null }
		})
		.mockResolvedValue({ kind: 'unavailable' });
	const choose = vi.fn();
	render(CatalogPicker, { label: 'Applications', load, choose });
	await fireEvent.click(await screen.findByRole('button', { name: 'Select Portal' }));
	expect(choose).toHaveBeenCalledWith({ id: 'one', name: 'Portal' });
	await fireEvent.click(screen.getByRole('button', { name: 'More options' }));
	await screen.findByRole('button', { name: 'Select Other' });
	expect(load).toHaveBeenLastCalledWith('', 'two');
	await fireEvent.input(screen.getByLabelText('Search Applications'), {
		target: { value: ' Missing ' }
	});
	await fireEvent.click(screen.getByRole('button', { name: 'Search options' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('could not be loaded');
	expect(load).toHaveBeenLastCalledWith('Missing', undefined);
	expect(screen.getByText('No matching options.')).toBeInTheDocument();
});
it('ignores a delayed response after unmount', async () => {
	let finish!: (v: unknown) => void;
	const load = vi.fn().mockImplementation(
		() =>
			new Promise((resolve) => {
				finish = resolve;
			})
	);
	const rendered = render(CatalogPicker, { label: 'Applications', load, choose: vi.fn() });
	await waitFor(() => expect(load).toHaveBeenCalled());
	rendered.unmount();
	finish({ kind: 'ready', data: { items: [], next: null } });
	await Promise.resolve();
	expect(screen.queryByRole('group')).not.toBeInTheDocument();
});
