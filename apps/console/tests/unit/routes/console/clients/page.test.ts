import { render, screen } from '@testing-library/svelte';
import { expect, it, vi, afterEach } from 'vitest';
import Page from '../../../../../src/routes/console/clients/+page.svelte';
afterEach(() => vi.unstubAllGlobals());
it('connects the static clients page to the protected catalog and marks its navigation', async () => {
	window.history.replaceState({}, '', '/?application_id=00000000-0000-0000-0000-000000000001');
	const fetcher = vi.fn().mockImplementation(() =>
		Promise.resolve(
			new Response(JSON.stringify({ items: [], next: null, policy_revision: '7' }), {
				status: 200
			})
		)
	);
	vi.stubGlobal('fetch', fetcher);
	render(Page);
	expect(await screen.findByRole('table')).toBeInTheDocument();
	expect(screen.getByRole('link', { name: 'Clients' })).toHaveAttribute('aria-current', 'page');
	expect(fetcher).toHaveBeenCalledWith(
		expect.stringContaining('/api/admin/catalog/clients?'),
		expect.objectContaining({ cache: 'no-store', credentials: 'same-origin' })
	);
});
