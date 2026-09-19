import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import Page from '../../../../src/routes/invitation/+page.svelte';
import { acceptInvitation } from '$lib/invitations';
import { replaceState } from '$app/navigation';
vi.mock('$app/navigation', () => ({ replaceState: vi.fn() }));
vi.mock('$app/state', () => ({ page: { state: {} } }));
vi.mock('$lib/invitations', async (original) => ({
	...(await original<object>()),
	acceptInvitation: vi.fn().mockResolvedValue('ok')
}));
afterEach(() => {
	cleanup();
	window.history.replaceState({}, '', '/');
	vi.clearAllMocks();
});
it('removes the secret fragment immediately and wires only explicit acceptance', async () => {
	const token = `iv1_${'a'.repeat(64)}`;
	window.history.replaceState({}, '', `/invitation#token=${token}`);
	render(Page);
	await screen.findByRole('form');
	expect(replaceState).toHaveBeenCalledWith('/invitation', {});
	expect(acceptInvitation).not.toHaveBeenCalled();
	for (const [label, value] of [
		['Email address', 'new@example.com'],
		['First name', 'New'],
		['Last name', 'Person'],
		['Password', 'a long test password'],
		['Confirm password', 'a long test password']
	])
		await fireEvent.input(screen.getByLabelText(label), { target: { value } });
	await fireEvent.submit(screen.getByRole('form'));
	expect(acceptInvitation).toHaveBeenCalledWith(fetch, {
		token,
		email: 'new@example.com',
		first_name: 'New',
		last_name: 'Person',
		password: 'a long test password'
	});
});
it('handles a link while already mounted, with no mutation for missing fragments', async () => {
	render(Page);
	await screen.findByText(/Open the invitation link/);
	expect(replaceState).not.toHaveBeenCalled();
	window.history.replaceState({}, '', `/invitation#token=iv1_${'a'.repeat(64)}`);
	window.dispatchEvent(new HashChangeEvent('hashchange'));
	await screen.findByRole('form');
	expect(replaceState).toHaveBeenCalledWith('/invitation', {});
	expect(acceptInvitation).not.toHaveBeenCalled();
});
