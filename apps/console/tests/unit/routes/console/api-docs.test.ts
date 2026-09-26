import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import ApiReferencePanel from '../../../../src/lib/components/admin/ApiReferencePanel.svelte';

const specification = {
	openapi: '3.2.1',
	info: { title: 'Protocol API', version: '0.1.0' },
	paths: {
		'/introspect': {
			post: {
				operationId: 'postIntrospection',
				summary: 'Check a credential',
				description: 'Authenticate the resource server.',
				tags: ['Tokens'],
				security: [{ ResourceServerBasic: [] }],
				requestBody: {
					content: {
						'application/x-www-form-urlencoded': {
							examples: {
								opaqueAccess: { value: { token: 'da_<64 lowercase hexadecimal characters>' } }
							}
						}
					}
				},
				responses: { '200': { description: 'Current credential state.' } }
			}
		},
		'/userinfo': {
			get: {
				operationId: 'getUserInfo',
				summary: '<img src=x onerror=alert(1)>',
				description: 'Return authorized claims.',
				tags: ['Tokens'],
				responses: { '200': { description: 'Claims.' } }
			}
		}
	}
};
const classification = {
	schema: 1,
	version: '0.1.0',
	entries: [
		{
			source: 'crates/adapters/src/token_http/mod.rs',
			function: 'router',
			method: 'POST',
			path: '/introspect',
			handler: 'introspect',
			surface: 'integration',
			operation_id: 'postIntrospection',
			rationale: 'Supported OIDC operation.'
		},
		{
			source: 'crates/adapters/src/token_http/mod.rs',
			function: 'router',
			method: 'GET',
			path: '/userinfo',
			handler: 'userinfo',
			surface: 'integration',
			operation_id: 'getUserInfo',
			rationale: 'Supported OIDC operation.'
		}
	]
};

afterEach(() => {
	vi.unstubAllGlobals();
	Reflect.deleteProperty(navigator, 'clipboard');
	window.history.replaceState(null, '', '/');
});

describe('API documentation page', () => {
	it('loads public local assets without cookies, searches, copies safe examples and offers downloads', async () => {
		const writeText = vi.fn().mockResolvedValue(undefined);
		Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
		const fetcher = vi
			.fn<typeof fetch>()
			.mockResolvedValueOnce(new Response(JSON.stringify(specification)))
			.mockResolvedValueOnce(new Response(JSON.stringify(classification)));
		vi.stubGlobal('fetch', fetcher);
		render(ApiReferencePanel);

		expect(await screen.findByRole('heading', { name: 'API documentation' })).toBeInTheDocument();
		expect(screen.getByText('Application release 0.1.0')).toBeInTheDocument();
		expect(fetcher).toHaveBeenNthCalledWith(1, '/reference/openapi-v1.json', {
			credentials: 'omit',
			cache: 'no-store',
			redirect: 'error'
		});
		expect(fetcher).toHaveBeenCalledTimes(2);
		expect(screen.getByRole('link', { name: 'Download OpenAPI specification' })).toHaveAttribute(
			'href',
			'/reference/openapi-v1.json'
		);
		expect(screen.getByRole('link', { name: 'Download route boundary inventory' })).toHaveAttribute(
			'download',
			'darkhorse-route-boundary.json'
		);
		expect(screen.getByText('<img src=x onerror=alert(1)>', { exact: true })).toBeInTheDocument();
		expect(document.querySelector('img[src="x"]')).toBeNull();

		await fireEvent.input(screen.getByRole('searchbox', { name: /Search supported operations/ }), {
			target: { value: 'introspect' }
		});
		expect(screen.getByText('/introspect')).toBeInTheDocument();
		expect(screen.queryByText('/userinfo')).not.toBeInTheDocument();
		await fireEvent.click(screen.getByText('/introspect'));
		await fireEvent.click(
			screen.getByRole('button', { name: 'Copy example for postIntrospection' })
		);
		await waitFor(() =>
			expect(writeText).toHaveBeenCalledWith(
				JSON.stringify({ token: 'da_<64 lowercase hexadecimal characters>' }, null, 2)
			)
		);
		expect(await screen.findByRole('status')).toHaveTextContent('Example copied.');
	});

	it('supports stable deep links and reports a safe failure with a retry', async () => {
		window.history.replaceState(null, '', '/console/api-docs#operation-getUserInfo');
		const fetcher = vi
			.fn<typeof fetch>()
			.mockResolvedValueOnce(new Response(JSON.stringify(specification)))
			.mockResolvedValueOnce(new Response(JSON.stringify(classification)));
		vi.stubGlobal('fetch', fetcher);
		const loadedView = render(ApiReferencePanel);
		await screen.findByRole('heading', { name: 'API documentation' });
		await waitFor(() =>
			expect(document.getElementById('operation-getUserInfo')).toHaveAttribute('open')
		);
		expect(screen.getByRole('button', { name: 'Copy link to getUserInfo' })).toBeInTheDocument();

		loadedView.unmount();
		vi.unstubAllGlobals();
		const failed = vi
			.fn<typeof fetch>()
			.mockResolvedValue(new Response('untrusted body', { status: 503 }));
		vi.stubGlobal('fetch', failed);
		const view = render(ApiReferencePanel);
		expect(await screen.findByRole('alert')).toHaveTextContent(
			'The bundled reference could not be loaded'
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Reload reference' }));
		await waitFor(() => expect(failed).toHaveBeenCalledTimes(4));
		view.unmount();
	});
});
