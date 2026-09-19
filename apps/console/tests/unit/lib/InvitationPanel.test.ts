import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import InvitationPanel from '$lib/components/InvitationPanel.svelte';
import type { InvitationInput, InvitationResult } from '$lib/invitations';
afterEach(cleanup);
function props() {
	return {
		accept: vi.fn<(input: InvitationInput) => Promise<InvitationResult>>().mockResolvedValue('ok'),
		takeToken: vi.fn<() => string | undefined>().mockReturnValue('proof'),
		watchToken: vi.fn<(changed: () => void) => () => void>().mockReturnValue(vi.fn())
	};
}
async function fill() {
	for (const [label, value] of [
		['Email address', 'new@example.com'],
		['First name', 'New'],
		['Last name', 'Person'],
		['Password', 'a long test password'],
		['Confirm password', 'a long test password']
	])
		await fireEvent.input(screen.getByLabelText(label), { target: { value } });
}
it('needs an email link and never submits on mount', async () => {
	const p = props();
	p.takeToken.mockReturnValue(undefined);
	render(InvitationPanel, p);
	await screen.findByText(/Open the invitation link/);
	expect(p.accept).not.toHaveBeenCalled();
	expect(screen.queryByRole('form')).toBeNull();
});
it('checks confirmation, clears passwords, blocks duplicate submits, and signs in separately', async () => {
	const p = props();
	let finish!: (r: InvitationResult) => void;
	p.accept.mockImplementation(
		() =>
			new Promise((resolve) => {
				finish = resolve;
			})
	);
	render(InvitationPanel, p);
	await screen.findByRole('form');
	await fill();
	await fireEvent.input(screen.getByLabelText('Confirm password'), {
		target: { value: 'different' }
	});
	await fireEvent.submit(screen.getByRole('form'));
	await screen.findByText('The passwords must match.');
	expect(p.accept).not.toHaveBeenCalled();
	await fireEvent.input(screen.getByLabelText('Confirm password'), {
		target: { value: 'a long test password' }
	});
	const form = screen.getByRole('form');
	await fireEvent.submit(form);
	await fireEvent.submit(form);
	expect(p.accept).toHaveBeenCalledTimes(1);
	expect(screen.getByLabelText('Password')).toHaveValue('');
	expect(screen.getByLabelText('Confirm password')).toHaveValue('');
	expect(p.accept).toHaveBeenCalledWith({
		token: 'proof',
		email: 'new@example.com',
		first_name: 'New',
		last_name: 'Person',
		password: 'a long test password'
	});
	p.watchToken.mock.calls[0][0]();
	finish('ok');
	await screen.findByText(/Your account is ready/);
	expect(screen.queryByRole('form')).toBeNull();
	expect(screen.getByRole('link', { name: 'Go to sign in' })).toHaveAttribute('href', '/');
});
it.each(['invalid', 'limited', 'unavailable'] as const)(
	'explains %s and prevents automatic replay',
	async (result) => {
		const p = props();
		p.accept.mockResolvedValue(result);
		render(InvitationPanel, p);
		await screen.findByRole('form');
		await fill();
		await fireEvent.submit(screen.getByRole('form'));
		await screen.findByRole('alert');
		expect(screen.queryByRole('form')).toBeNull();
		expect(p.accept).toHaveBeenCalledTimes(1);
		expect(screen.getByRole('alert').textContent).toMatch(
			result === 'invalid'
				? /could not be accepted/
				: result === 'limited'
					? /attempt limit/
					: /could not confirm/
		);
	}
);
it('accepts a newly opened fragment and removes its listener on unmount', async () => {
	const p = props();
	p.takeToken.mockReturnValue(undefined);
	const rendered = render(InvitationPanel, p);
	await screen.findByText(/Open the invitation link/);
	p.takeToken.mockReturnValue('new proof');
	p.watchToken.mock.calls[0][0]();
	await waitFor(() => expect(screen.getByRole('form')).toBeInTheDocument());
	rendered.unmount();
	expect(p.watchToken.mock.results[0].value).toHaveBeenCalled();
});
it('does not read or replace a fragment after immediate unmount', async () => {
	const p = props();
	const rendered = render(InvitationPanel, p);
	rendered.unmount();
	await Promise.resolve();
	await Promise.resolve();
	expect(p.takeToken).not.toHaveBeenCalled();
});
