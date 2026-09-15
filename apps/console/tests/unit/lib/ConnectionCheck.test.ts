import { fireEvent, render, screen } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import ConnectionCheck from '../../../src/lib/components/ConnectionCheck.svelte';
import type { Connection } from '../../../src/lib/health';

it('announces pending and successful checks and prevents duplicate clicks', async () => {
	let finish!: (value: Connection) => void;
	const check = vi.fn(
		() =>
			new Promise<Connection>((resolve) => {
				finish = resolve;
			})
	);
	render(ConnectionCheck, { check });
	const button = screen.getByRole('button', { name: 'Check connection' });
	await fireEvent.click(button);
	expect(check).toHaveBeenCalledTimes(1);
	expect(button).toBeDisabled();
	expect(screen.getByRole('status')).toHaveTextContent('Checking connection');
	finish('ready');
	expect(await screen.findByText('Service reachable')).toBeInTheDocument();
	expect(button).toBeEnabled();
});

it('offers another attempt after a failed check', async () => {
	render(ConnectionCheck, { check: vi.fn().mockResolvedValue('unavailable') });
	await fireEvent.click(screen.getByRole('button', { name: 'Check connection' }));
	expect(await screen.findByText('Service unavailable. Try again.')).toBeInTheDocument();
	expect(screen.getByRole('button')).toBeEnabled();
});
