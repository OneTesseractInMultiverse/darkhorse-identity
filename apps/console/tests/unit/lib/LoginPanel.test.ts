import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import LoginPanel from '../../../src/lib/components/LoginPanel.svelte';
function props() {
	return {
		signIn: vi.fn().mockResolvedValue({ kind: 'signed-in', name: 'Ada' }),
		checkSession: vi.fn().mockResolvedValue({ kind: 'signed-out' }),
		signOut: vi.fn().mockResolvedValue(true)
	};
}

it('shows dependency failures and throttling without exposing account details', async () => {
	const p = props();
	p.checkSession.mockResolvedValue({ kind: 'unavailable' });
	p.signIn.mockResolvedValue({ kind: 'limited' });
	render(LoginPanel, p);
	expect(await screen.findByRole('alert')).toHaveTextContent('temporarily unavailable');
	await fireEvent.input(screen.getByLabelText('Email address'), {
		target: { value: 'a@example.com' }
	});
	await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'test-only' } });
	await fireEvent.submit(screen.getByRole('form', { name: 'Sign in' }));
	await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('Too many attempts'));
	expect(screen.getByLabelText('Password')).toHaveValue('');
});
it('signs in with labelled fields, clears the password and signs out', async () => {
	const p = props();
	render(LoginPanel, p);
	const submit = await screen.findByRole('button', { name: 'Sign in' });
	await fireEvent.input(screen.getByLabelText('Email address'), {
		target: { value: 'a@example.com' }
	});
	await fireEvent.input(screen.getByLabelText('Password'), {
		target: { value: 'test-only password' }
	});
	await fireEvent.click(submit);
	expect(await screen.findByRole('heading', { name: 'Welcome, Ada.' })).toBeInTheDocument();
	expect(p.signIn).toHaveBeenCalledWith('a@example.com', 'test-only password');
	await fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
	expect(await screen.findByLabelText('Password')).toHaveValue('');
});
it('focuses generic failure and clears the password', async () => {
	const p = props();
	p.signIn.mockResolvedValue({ kind: 'signed-out' });
	render(LoginPanel, p);
	await screen.findByRole('button', { name: 'Sign in' });
	await fireEvent.input(screen.getByLabelText('Email address'), {
		target: { value: 'a@example.com' }
	});
	await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'wrong' } });
	await fireEvent.submit(screen.getByRole('form', { name: 'Sign in' }));
	const error = await screen.findByRole('alert');
	expect(error).toHaveTextContent('Unable to sign in with those credentials.');
	await waitFor(() => expect(error).toHaveFocus());
	expect(screen.getByLabelText('Password')).toHaveValue('');
});
it('restores a session and keeps it visible if logout fails', async () => {
	const p = props();
	p.checkSession.mockResolvedValue({ kind: 'signed-in', name: 'Ada' });
	p.signOut.mockResolvedValue(false);
	render(LoginPanel, { ...p, showSecurityLink: true });
	await fireEvent.click(await screen.findByRole('button', { name: 'Sign out' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('Sign out could not be confirmed');
	expect(screen.getByRole('heading', { name: 'Welcome, Ada.' })).toBeInTheDocument();
});
