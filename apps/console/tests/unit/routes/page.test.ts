import { fireEvent, render, screen } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import Page from '../../../src/routes/+page.svelte';
import { authenticate, currentSession, endSession } from '../../../src/lib/authentication';
vi.mock('../../../src/lib/authentication', () => ({
	currentSession: vi.fn().mockResolvedValue({ kind: 'signed-out' }),
	authenticate: vi.fn().mockResolvedValue({ kind: 'signed-in', name: 'Ada' }),
	endSession: vi.fn().mockResolvedValue(true)
}));
it('connects the static portal to the Rust session boundary', async () => {
	render(Page);
	expect(await screen.findByRole('button', { name: 'Sign in' })).toBeInTheDocument();
	expect(screen.getByRole('link', { name: 'Darkhorse home' })).toHaveAttribute('href', '/');
	expect(currentSession).toHaveBeenCalledTimes(1);
	await fireEvent.input(screen.getByLabelText('Email address'), {
		target: { value: 'a@example.com' }
	});
	await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'test-only' } });
	await fireEvent.submit(screen.getByRole('form', { name: 'Sign in' }));
	expect(await screen.findByRole('heading', { name: 'Welcome, Ada.' })).toBeInTheDocument();
	expect(authenticate).toHaveBeenCalledWith(fetch, 'a@example.com', 'test-only');
	expect(screen.getByRole('navigation', { name: 'Account tools' })).toBeInTheDocument();
	expect(screen.getByRole('link', { name: 'Open console' })).toHaveAttribute(
		'href',
		'/console/users'
	);
	expect(screen.getByRole('link', { name: 'Manage sessions' })).toHaveAttribute(
		'href',
		'/security/sessions'
	);
	await fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
	expect(await screen.findByRole('button', { name: 'Sign in' })).toBeInTheDocument();
	expect(screen.queryByRole('navigation', { name: 'Account tools' })).not.toBeInTheDocument();
	expect(endSession).toHaveBeenCalledWith(fetch);
});
