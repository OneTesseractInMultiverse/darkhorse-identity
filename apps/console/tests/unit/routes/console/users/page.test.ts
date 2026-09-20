import { render, screen } from '@testing-library/svelte';
import { expect, it, vi, afterEach } from 'vitest';
import Page from '../../../../../src/routes/console/users/+page.svelte';
afterEach(() => vi.unstubAllGlobals());
it('connects the static shell to the protected Rust directory with accessible navigation', async () => {
	const fetcher = vi
		.fn()
		.mockResolvedValue(
			new Response(
				JSON.stringify({ actor: '00000000-0000-0000-0000-000000000001', items: [], next: null }),
				{ status: 200 }
			)
		);
	vi.stubGlobal('fetch', fetcher);
	render(Page);
	expect(await screen.findByRole('table')).toBeInTheDocument();
	expect(screen.getByRole('heading', { name: 'User directory' })).toBeInTheDocument();
	expect(screen.getByRole('navigation', { name: 'Management' })).toBeInTheDocument();
	expect(screen.getByRole('link', { name: 'User directory' })).toHaveAttribute(
		'href',
		'/console/users'
	);
	expect(screen.getByRole('link', { name: 'Skip to console' })).toHaveAttribute(
		'href',
		'#directory-content'
	);
	expect(fetcher).toHaveBeenCalledWith(
		'/api/admin/users?limit=25',
		expect.objectContaining({ credentials: 'same-origin', cache: 'no-store' })
	);
	expect(screen.getByText(/No users match/)).toBeInTheDocument();
});
