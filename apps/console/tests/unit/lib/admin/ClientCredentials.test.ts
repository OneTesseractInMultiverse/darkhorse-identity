import { render, screen, fireEvent } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import ClientCredentials from '../../../../src/lib/components/admin/ClientCredentials.svelte';
it('keeps retirement separate from rotation and honors cancel and overlap input', async () => {
	const save = vi.fn();
	render(ClientCredentials, {
		client: {
			kind: 'client',
			id: 'client',
			name: 'Web',
			application_id: 'app',
			revision: '17',
			secrets: [
				{ id: 'first', created_ms: 1, expires_ms: null },
				{ id: 'old', created_ms: 1, expires_ms: 1000 }
			]
		},
		pending: false,
		save
	});
	await fireEvent.click(screen.getAllByRole('button', { name: 'Retire secret' })[0]);
	expect(save).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'Cancel credential change' }));
	expect(screen.queryByRole('button', { name: 'Confirm retirement' })).not.toBeInTheDocument();
	await fireEvent.click(screen.getAllByRole('button', { name: 'Retire secret' })[1]);
	await fireEvent.submit(
		screen.getByRole('button', { name: 'Confirm retirement' }).closest('form')!
	);
	expect(save).toHaveBeenCalledWith({
		operation: 'retire_secret',
		application_id: 'app',
		client_id: 'client',
		secret_id: 'old',
		revision: '17'
	});
	await fireEvent.click(screen.getByRole('button', { name: 'Rotate secret' }));
	await fireEvent.input(screen.getByLabelText('Previous secret overlap (seconds)'), {
		target: { value: '300' }
	});
	await fireEvent.submit(screen.getByRole('button', { name: 'Confirm rotation' }).closest('form')!);
	expect(save).toHaveBeenLastCalledWith({
		operation: 'rotate_secret',
		application_id: 'app',
		client_id: 'client',
		revision: '17',
		overlap_seconds: 300
	});
});
