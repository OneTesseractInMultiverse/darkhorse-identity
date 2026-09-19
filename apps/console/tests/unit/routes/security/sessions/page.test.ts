import { fireEvent, render, screen } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import Page from '../../../../../src/routes/security/sessions/+page.svelte';
import { readSessions, terminateSession } from '../../../../../src/lib/sessions';
vi.mock('../../../../../src/lib/sessions', async (original) => ({
	...(await original<object>()),
	readSessions: vi.fn().mockResolvedValue({
		kind: 'ready',
		page: {
			current: '00000000-0000-0000-0000-000000000001',
			items: [
				{
					id: '00000000-0000-0000-0000-000000000001',
					created_ms: 1000,
					seen_ms: 2000,
					expires_ms: 3000,
					status: 'active'
				}
			],
			next: null
		}
	}),
	terminateSession: vi.fn().mockResolvedValue({ kind: 'signed-out' })
}));
it('wires the static session page to the protected Rust API', async () => {
	HTMLDialogElement.prototype.showModal = function () {
		this.open = true;
	};
	HTMLDialogElement.prototype.close = function () {
		this.open = false;
		this.dispatchEvent(new Event('close'));
	};
	render(Page);
	expect(screen.getByRole('link', { name: 'Darkhorse home' })).toHaveAttribute('href', '/');
	await fireEvent.click(await screen.findByRole('button', { name: 'End this session' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm end session' }));
	expect(await screen.findByRole('link', { name: 'Sign in' })).toHaveAttribute('href', '/');
	expect(readSessions).toHaveBeenCalledExactlyOnceWith(fetch, undefined);
	expect(terminateSession).toHaveBeenCalledExactlyOnceWith(
		fetch,
		'00000000-0000-0000-0000-000000000001'
	);
});
