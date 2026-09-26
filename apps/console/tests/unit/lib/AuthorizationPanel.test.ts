import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import AuthorizationPanel from '../../../src/lib/components/AuthorizationPanel.svelte';
const pending = {
	kind: 'pending',
	request_id: 'a'.repeat(64),
	ui_locale: null,
	client_name: 'Calendar',
	scopes: ['openid', 'events.read'],
	resource: 'urn:calendar',
	status: 'consent'
};
function props() {
	return {
		load: vi.fn().mockResolvedValue(pending),
		decide: vi.fn().mockResolvedValue({ ...pending, status: 'ready' }),
		signIn: vi.fn().mockResolvedValue({ kind: 'signed-in', name: 'Ada' }),
		signOut: vi.fn().mockResolvedValue(true),
		navigate: vi.fn()
	};
}
it('shows requested access and approves only the reviewed request', async () => {
	const p = props();
	render(AuthorizationPanel, p);
	expect(await screen.findByRole('heading', { name: 'Connect Calendar?' })).toBeInTheDocument();
	expect(screen.getByText('events.read')).toBeInTheDocument();
	await fireEvent.click(screen.getByRole('button', { name: 'Allow connection' }));
	expect(p.decide).toHaveBeenCalledWith(pending.request_id, 'approve');
	expect(await screen.findByRole('heading', { name: 'Unable to connect' })).toBeInTheDocument();
});
it('cancels through the server and follows only its validated return target', async () => {
	const p = props();
	p.decide.mockResolvedValue({
		kind: 'redirect',
		url: 'https://client.example/callback?error=access_denied'
	});
	render(AuthorizationPanel, p);
	await fireEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
	expect(p.decide).toHaveBeenCalledWith(pending.request_id, 'deny');
	await waitFor(() =>
		expect(p.navigate).toHaveBeenCalledWith('https://client.example/callback?error=access_denied')
	);
});
it('requests authentication before consent and reloads after sign-in', async () => {
	const p = props();
	p.load.mockResolvedValueOnce({ ...pending, status: 'login' });
	render(AuthorizationPanel, p);
	await fireEvent.input(await screen.findByLabelText('Email address'), {
		target: { value: 'ada@example.com' }
	});
	await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'test-only' } });
	await fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
	expect(await screen.findByRole('heading', { name: 'Connect Calendar?' })).toBeInTheDocument();
	expect(p.load).toHaveBeenCalledTimes(2);
});
it('keeps failed authentication at the login boundary and reports expired requests', async () => {
	const p = props();
	p.load.mockResolvedValueOnce({ ...pending, status: 'login' });
	p.signIn.mockResolvedValue({ kind: 'signed-out' });
	render(AuthorizationPanel, p);
	await fireEvent.input(await screen.findByLabelText('Email address'), {
		target: { value: 'ada@example.com' }
	});
	await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'wrong' } });
	await fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('Unable to sign in');
	expect(p.load).toHaveBeenCalledTimes(1);
});
it('renders unavailable requests without an approval action', async () => {
	const p = props();
	p.load.mockResolvedValue({ kind: 'unavailable' });
	render(AuthorizationPanel, p);
	expect(await screen.findByRole('heading', { name: 'Unable to connect' })).toBeInTheDocument();
	expect(screen.queryByRole('button', { name: 'Allow connection' })).not.toBeInTheDocument();
});
it('ignores a late redirect after leaving the authorization page', async () => {
	const p = props();
	let finish!: (value: { kind: 'redirect'; url: string }) => void;
	p.load.mockReturnValue(
		new Promise((resolve) => {
			finish = resolve;
		})
	);
	const view = render(AuthorizationPanel, p);
	view.unmount();
	finish({ kind: 'redirect', url: 'https://client.example/callback?code=late' });
	await new Promise((resolve) => setTimeout(resolve, 0));
	expect(p.navigate).not.toHaveBeenCalled();
});
