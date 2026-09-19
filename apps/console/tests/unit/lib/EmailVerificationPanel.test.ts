import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import EmailVerificationPanel from '$lib/components/EmailVerificationPanel.svelte';
import type { EmailStatus, EmailResult } from '$lib/email-verification';
afterEach(cleanup);
function props() {
	return {
		read: vi
			.fn<() => Promise<EmailStatus>>()
			.mockResolvedValue({ kind: 'ready', email: 'one@example.com', verified: false }),
		request: vi.fn<() => Promise<EmailResult>>().mockResolvedValue('ok'),
		confirm: vi.fn<(token: string) => Promise<EmailResult>>().mockResolvedValue('ok'),
		takeToken: vi.fn<() => string | undefined>().mockReturnValue(undefined),
		signIn: vi.fn().mockResolvedValue({ kind: 'signed-in', name: 'One' }),
		checkSession: vi.fn().mockResolvedValue({ kind: 'signed-out' }),
		signOut: vi.fn().mockResolvedValue(true)
	};
}
it('queues only after an explicit request and reports delivery as queued', async () => {
	const p = props();
	render(EmailVerificationPanel, p);
	await screen.findByText('Email not verified');
	expect(p.request).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'Send verification email' }));
	await screen.findByText(/A verification message has been queued/);
	expect(p.request).toHaveBeenCalledTimes(1);
});
it('keeps the proof in memory, never confirms on mount, and reconciles a lost response', async () => {
	const p = props();
	p.takeToken.mockReturnValue('proof');
	p.confirm.mockResolvedValue('unavailable');
	render(EmailVerificationPanel, p);
	await screen.findByText('Email not verified');
	expect(p.confirm).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm email' }));
	await screen.findByText(/The result could not be confirmed/);
	expect(p.confirm).toHaveBeenCalledWith('proof');
	expect(screen.getByRole('button', { name: 'Confirm email' })).toBeDisabled();
	p.read.mockResolvedValue({ kind: 'ready', email: 'one@example.com', verified: true });
	await fireEvent.click(screen.getByRole('button', { name: 'Refresh status' }));
	await screen.findByText('Email verified');
	expect(screen.queryByRole('button', { name: 'Confirm email' })).toBeNull();
});
it.each(['invalid', 'limited', 'signed-out'] as const)(
	'explains %s without claiming success',
	async (result) => {
		const p = props();
		p.takeToken.mockReturnValue('proof');
		p.confirm.mockResolvedValue(result);
		render(EmailVerificationPanel, p);
		await screen.findByText('Email not verified');
		await fireEvent.click(screen.getByRole('button', { name: 'Confirm email' }));
		await waitFor(() =>
			expect(screen.getByRole('status').textContent).toMatch(
				result === 'invalid' ? /expired/ : result === 'limited' ? /Please wait/ : /Sign in/
			)
		);
	}
);
it('confirms, refreshes verified state, and allows changing accounts', async () => {
	const p = props();
	p.takeToken.mockReturnValue('proof');
	render(EmailVerificationPanel, p);
	await screen.findByText('Email not verified');
	p.read.mockResolvedValue({ kind: 'ready', email: 'one@example.com', verified: true });
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm email' }));
	await screen.findByText('Your email has been verified.');
	p.signOut.mockResolvedValue(false);
	await fireEvent.click(screen.getByRole('button', { name: 'Sign in with another account' }));
	await screen.findByText(/Sign out could not be confirmed/);
	p.signOut.mockResolvedValue(true);
	p.read.mockResolvedValue({ kind: 'signed-out' });
	await fireEvent.click(screen.getByRole('button', { name: 'Sign in with another account' }));
	await screen.findByRole('form', { name: 'Sign in' });
});
it.each(['disabled', 'unavailable'] as const)('explains %s status', async (kind) => {
	const p = props();
	p.read.mockResolvedValue({ kind });
	render(EmailVerificationPanel, p);
	await screen.findByText(kind === 'disabled' ? /not enabled/ : /temporarily unavailable/);
});
it('allows signing in while retaining a link in memory', async () => {
	const p = props();
	p.read.mockResolvedValueOnce({ kind: 'signed-out' });
	p.takeToken.mockReturnValue('proof');
	render(EmailVerificationPanel, p);
	const email = await screen.findByLabelText('Email address');
	await waitFor(() => expect(email).not.toBeDisabled());
	await fireEvent.input(email, { target: { value: 'one@example.com' } });
	await fireEvent.input(screen.getByLabelText('Password'), {
		target: { value: 'test password sufficient' }
	});
	await fireEvent.submit(screen.getByRole('form', { name: 'Sign in' }));
	await screen.findByRole('button', { name: 'Confirm email' });
	expect(p.confirm).not.toHaveBeenCalled();
});
it('does not read a proof or account after immediate unmount', async () => {
	const p = props();
	const rendered = render(EmailVerificationPanel, p);
	rendered.unmount();
	await Promise.resolve();
	await Promise.resolve();
	expect(p.takeToken).not.toHaveBeenCalled();
	expect(p.read).not.toHaveBeenCalled();
});
