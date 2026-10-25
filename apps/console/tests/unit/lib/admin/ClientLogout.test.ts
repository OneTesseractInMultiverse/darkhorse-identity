import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import ClientLogout from '../../../../src/lib/components/admin/ClientLogout.svelte';
afterEach(() => vi.unstubAllGlobals());
it('explains the setting, preserves revisions and reports uncertain delivery configuration writes', async () => {
	const configuration = {
		revision: '7',
		backchannel_logout_uri: null,
		backchannel_logout_session_required: true
	};
	const fetcher = vi
		.fn<typeof fetch>()
		.mockResolvedValueOnce(new Response(JSON.stringify({ configuration, deliveries: [] })))
		.mockRejectedValueOnce(new Error('lost response'));
	vi.stubGlobal('fetch', fetcher);
	render(ClientLogout, {
		client: { kind: 'client', id: 'client', name: 'Web', application_id: 'app', revision: '0' }
	});
	await fireEvent.click(await screen.findByRole('button', { name: 'Configure logout' }));
	const input = screen.getByLabelText('Logout notification URL');
	expect(input).toHaveAccessibleDescription(/Leave empty to disable/);
	await fireEvent.input(input, { target: { value: 'https://application.example/logout' } });
	await fireEvent.submit(
		screen.getByRole('button', { name: 'Save logout settings' }).closest('form')!
	);
	await waitFor(() => expect(screen.getByRole('alert')).toBeInTheDocument());
	expect(fetcher).toHaveBeenCalledTimes(2);
	expect(JSON.parse(String(fetcher.mock.calls[1][1]?.body))).toEqual({
		...configuration,
		backchannel_logout_uri: 'https://application.example/logout'
	});
	expect(screen.getByRole('button', { name: 'Save logout settings' })).not.toBeDisabled();
});
