import { render, screen } from '@testing-library/svelte';
import { expect, it, vi, afterEach } from 'vitest';
import Page from '../../../../../src/routes/console/api-docs/+page.svelte';

afterEach(() => vi.unstubAllGlobals());

it('exposes the localized console route and marks its navigation entry active', async () => {
	const specification = {
		openapi: '3.2.1',
		info: { title: 'Protocol API', version: '0.1.0' },
		paths: {
			'/jwks': {
				get: {
					operationId: 'getJwks',
					summary: 'Read signing keys',
					description: 'Public signing keys only.',
					tags: ['Discovery'],
					responses: { '200': { description: 'Keys.' } }
				}
			}
		}
	};
	const classification = {
		schema: 1,
		version: '0.1.0',
		entries: [
			{
				source: 'crates/adapters/src/provider_http/mod.rs',
				function: 'router',
				method: 'GET',
				path: '/jwks',
				handler: 'jwks',
				surface: 'integration',
				operation_id: 'getJwks',
				rationale: 'Supported operation.'
			}
		]
	};
	const fetcher = vi
		.fn<typeof fetch>()
		.mockResolvedValueOnce(new Response(JSON.stringify(specification)))
		.mockResolvedValueOnce(new Response(JSON.stringify(classification)));
	vi.stubGlobal('fetch', fetcher);
	render(Page);

	expect(await screen.findByRole('heading', { name: 'API documentation' })).toBeInTheDocument();
	expect(screen.getByRole('link', { name: 'API documentation' })).toHaveAttribute(
		'aria-current',
		'page'
	);
	expect(document.title).toBe('API documentation — Darkhorse');
	expect(fetcher).toHaveBeenCalledTimes(2);
});
