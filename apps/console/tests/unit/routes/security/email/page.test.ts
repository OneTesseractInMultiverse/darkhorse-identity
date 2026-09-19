import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import Page from '../../../../../src/routes/security/email/+page.svelte';
import { emailStatus, requestEmail, confirmEmail } from '$lib/email-verification';
import { authenticate, currentSession, endSession } from '$lib/authentication';
import { replaceState } from '$app/navigation';
vi.mock('$app/navigation', () => ({ replaceState: vi.fn() }));
vi.mock('$app/state', () => ({ page: { state: {} } }));
vi.mock('$lib/email-verification', async (original) => ({
	...(await original<object>()),
	emailStatus: vi.fn(),
	requestEmail: vi.fn().mockResolvedValue('ok'),
	confirmEmail: vi.fn().mockResolvedValue('ok')
}));
vi.mock('$lib/authentication', () => ({
	authenticate: vi.fn().mockResolvedValue({ kind: 'signed-in', name: 'One' }),
	currentSession: vi.fn().mockResolvedValue({ kind: 'signed-out' }),
	endSession: vi.fn().mockResolvedValue(true)
}));
afterEach(() => {
	cleanup();
	window.history.replaceState({}, '', '/');
	vi.clearAllMocks();
});
it('removes the fragment before reads and wires only explicit confirmation', async () => {
	const token = `ev1_${'a'.repeat(64)}`;
	window.history.replaceState({}, '', `/security/email#token=${token}`);
	vi.mocked(emailStatus).mockResolvedValue({
		kind: 'ready',
		email: 'one@example.com',
		verified: false
	});
	render(Page);
	await screen.findByRole('button', { name: 'Confirm email' });
	expect(replaceState).toHaveBeenCalledWith('/security/email', {});
	expect(confirmEmail).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm email' }));
	expect(confirmEmail).toHaveBeenCalledWith(fetch, token);
});
it('wires status, request and account switching to the Rust API', async () => {
	vi.mocked(emailStatus).mockResolvedValue({
		kind: 'ready',
		email: 'one@example.com',
		verified: false
	});
	render(Page);
	await fireEvent.click(await screen.findByRole('button', { name: 'Send verification email' }));
	expect(requestEmail).toHaveBeenCalledWith(fetch);
	await waitFor(() =>
		expect(screen.getByRole('button', { name: 'Refresh status' })).not.toBeDisabled()
	);
	vi.mocked(emailStatus).mockResolvedValue({ kind: 'signed-out' });
	await fireEvent.click(screen.getByRole('button', { name: 'Sign in with another account' }));
	expect(endSession).toHaveBeenCalledWith(fetch);
	const email = await screen.findByLabelText('Email address');
	await waitFor(() => expect(email).not.toBeDisabled());
	expect(currentSession).toHaveBeenCalledWith(fetch);
	vi.mocked(emailStatus).mockResolvedValue({
		kind: 'ready',
		email: 'one@example.com',
		verified: false
	});
	await fireEvent.input(email, { target: { value: 'one@example.com' } });
	await fireEvent.input(screen.getByLabelText('Password'), {
		target: { value: 'a sufficiently long password' }
	});
	await fireEvent.submit(screen.getByRole('form', { name: 'Sign in' }));
	expect(authenticate).toHaveBeenCalledWith(
		fetch,
		'one@example.com',
		'a sufficiently long password'
	);
});

it('accepts a link opened while the same page is already mounted', async () => {
	vi.mocked(emailStatus).mockResolvedValue({
		kind: 'ready',
		email: 'one@example.com',
		verified: false
	});
	render(Page);
	await screen.findByRole('button', { name: 'Send verification email' });
	const token = `ev1_${'b'.repeat(64)}`;
	window.history.replaceState({}, '', `/security/email#token=${token}`);
	window.dispatchEvent(new HashChangeEvent('hashchange'));
	await screen.findByRole('button', { name: 'Confirm email' });
	expect(replaceState).toHaveBeenCalledWith('/security/email', {});
	expect(confirmEmail).not.toHaveBeenCalled();
});
