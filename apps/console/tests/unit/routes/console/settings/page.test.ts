import { render, screen } from '@testing-library/svelte';
import { it, expect, vi, afterEach } from 'vitest';
import Page from '../../../../../src/routes/console/settings/+page.svelte';
afterEach(() => vi.unstubAllGlobals());
it('connects protected settings and marks the selected navigation entry', async () => {
	const fetcher = vi.fn().mockResolvedValue(
		new Response(
			JSON.stringify({
				revision: '0',
				logo: false,
				background: false,
				storage_enabled: false,
				bucket: null
			})
		)
	);
	vi.stubGlobal('fetch', fetcher);
	render(Page);
	await screen.findByText(/Image storage is disabled/);
	expect(screen.getByRole('link', { name: 'System settings' })).toHaveAttribute(
		'aria-current',
		'page'
	);
	expect(fetcher).toHaveBeenCalledWith(
		'/api/admin/branding',
		expect.objectContaining({ credentials: 'same-origin', cache: 'no-store' })
	);
});
