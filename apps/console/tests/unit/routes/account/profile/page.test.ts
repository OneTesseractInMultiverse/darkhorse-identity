import { render, screen, fireEvent } from '@testing-library/svelte';
import { it, expect, vi, afterEach, beforeEach } from 'vitest';
import Page from '../../../../../src/routes/account/profile/+page.svelte';
const profile = {
	id: '00000000-0000-0000-0000-000000000001',
	revision: '0',
	preferred_locale: null,
	email: 'person@example.com',
	active: true,
	email_verified: false,
	first_name: 'Ana',
	second_name: '',
	last_name: 'Guzmán',
	second_last_name: '',
	country: '',
	calling_code: '',
	national_number: '',
	bio: ''
};
afterEach(() => vi.unstubAllGlobals());
beforeEach(() => {
	HTMLDialogElement.prototype.showModal = function () {
		this.open = true;
	};
});
it('connects profile and image workflows to the protected Rust endpoints', async () => {
	const fetcher = vi
		.fn()
		.mockImplementation((path: string) =>
			Promise.resolve(
				new Response(JSON.stringify(path.endsWith('/picture') ? { revision: '1' } : profile))
			)
		);
	vi.stubGlobal('fetch', fetcher);
	render(Page);
	await screen.findByText('person@example.com (unverified)');
	expect(screen.getByRole('link', { name: 'DARKHORSE' })).toHaveAttribute('href', '/');
	await fireEvent.click(screen.getByRole('button', { name: 'Change picture' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Remove image' }));
	expect(fetcher).toHaveBeenCalledWith(
		'/api/profiles/me/picture',
		expect.objectContaining({
			method: 'DELETE',
			headers: expect.objectContaining({ 'x-darkhorse-csrf': '1' })
		})
	);
});
