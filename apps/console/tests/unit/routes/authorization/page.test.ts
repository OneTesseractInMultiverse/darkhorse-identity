import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import Page from '../../../../src/routes/authorization/+page.svelte';
import { loadAuthorization, decideAuthorization } from '../../../../src/lib/authorization';
import { authenticate } from '../../../../src/lib/authentication';
vi.mock('$app/state', () => ({
	page: { url: new URL('https://identity.example/authorization?request=' + 'a'.repeat(64)) }
}));
vi.mock('../../../../src/lib/authentication', () => ({
	authenticate: vi.fn().mockResolvedValue({ kind: 'signed-in', name: 'Ada' }),
	endSession: vi.fn().mockResolvedValue(true)
}));
vi.mock('../../../../src/lib/authorization', () => ({
	loadAuthorization: vi.fn(),
	decideAuthorization: vi.fn()
}));
it('connects sign-in, displayed consent and cancellation to the Rust transport ports', async () => {
	const pending = {
		kind: 'pending' as const,
		request_id: 'a'.repeat(64),
		ui_locale: null,
		client_name: 'Calendar',
		scopes: ['openid'],
		resource: null,
		status: 'consent' as const
	};
	vi.mocked(loadAuthorization)
		.mockResolvedValueOnce({ ...pending, status: 'login' })
		.mockResolvedValue(pending);
	vi.mocked(decideAuthorization)
		.mockResolvedValueOnce({ ...pending, status: 'ready' })
		.mockResolvedValue({ kind: 'redirect', url: '#returned' });
	render(Page);
	await fireEvent.input(await screen.findByLabelText('Email address'), {
		target: { value: 'ada@example.com' }
	});
	await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'test-only' } });
	await fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
	await fireEvent.click(await screen.findByRole('button', { name: 'Allow connection' }));
	expect(authenticate).toHaveBeenCalledWith(fetch, 'ada@example.com', 'test-only');
	expect(decideAuthorization).toHaveBeenCalledWith(
		fetch,
		pending.request_id,
		pending.request_id,
		'approve'
	);
	await fireEvent.click(await screen.findByRole('button', { name: 'Cancel connection' }));
	await waitFor(() => expect(window.location.hash).toBe('#returned'));
	expect(decideAuthorization).toHaveBeenLastCalledWith(
		fetch,
		pending.request_id,
		pending.request_id,
		'deny'
	);
});
