import { render, screen } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import Page from '../../../../../src/routes/console/policies/+page.svelte';

const applicationId = '10000000-0000-4000-8000-000000000001';
const app = {
	kind: 'application',
	id: applicationId,
	name: 'Audit portal',
	active: true,
	revision: '3',
	owner_id: '20000000-0000-4000-8000-000000000002',
	owner_email: 'owner@example.test'
};
afterEach(() => vi.unstubAllGlobals());

it('loads a deep-linked application map through the protected no-store endpoint and reports oversized data without a partial graph', async () => {
	window.history.replaceState({}, '', `/console/policies?application_id=${applicationId}`);
	const fetcher = vi.fn().mockImplementation((path: string) => {
		if (String(path).startsWith('/api/admin/catalog/applications?'))
			return Promise.resolve(
				new Response(JSON.stringify({ items: [app], next: null, policy_revision: '3' }), {
					status: 200
				})
			);
		return Promise.resolve(new Response('', { status: 413 }));
	});
	vi.stubGlobal('fetch', fetcher);
	render(Page);
	expect(
		await screen.findByText(
			'This application policy is larger than the safe visualization limit. No partial graph was returned.'
		)
	).toBeInTheDocument();
	expect(screen.getByRole('link', { name: 'Policy map' })).toHaveAttribute('aria-current', 'page');
	expect(fetcher).toHaveBeenCalledWith(
		`/api/admin/console/applications/${applicationId}/policy-map`,
		expect.objectContaining({ method: 'GET', credentials: 'same-origin', cache: 'no-store' })
	);
	expect(screen.queryByRole('region', { name: /policy graph/i })).not.toBeInTheDocument();
});

it('rejects a malformed deep link before calling the policy endpoint', async () => {
	window.history.replaceState({}, '', '/console/policies?application_id=not-a-uuid');
	const fetcher = vi
		.fn()
		.mockResolvedValue(
			new Response(JSON.stringify({ items: [], next: null, policy_revision: '3' }), { status: 200 })
		);
	vi.stubGlobal('fetch', fetcher);
	render(Page);
	expect(
		await screen.findByText(
			'The application ID in the page address is invalid. Choose an application from the directory.'
		)
	).toBeInTheDocument();
	expect(fetcher).toHaveBeenCalledTimes(1);
});
