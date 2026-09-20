import { render, screen } from '@testing-library/svelte';
import { it, expect, vi } from 'vitest';
import Page from '../../../../../src/routes/security/keys/+page.svelte';
it('connects the static account page to the Rust key API', async () => {
	const fetcher = vi
		.fn()
		.mockResolvedValue(new Response(JSON.stringify({ items: [], next: null })));
	vi.stubGlobal('fetch', fetcher);
	try {
		render(Page);
		expect(await screen.findByText('You have no API keys yet.')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Darkhorse home' })).toHaveAttribute('href', '/');
		expect(fetcher).toHaveBeenCalledWith(
			'/api/security/keys',
			expect.objectContaining({ credentials: 'same-origin', cache: 'no-store' })
		);
	} finally {
		vi.unstubAllGlobals();
	}
});
