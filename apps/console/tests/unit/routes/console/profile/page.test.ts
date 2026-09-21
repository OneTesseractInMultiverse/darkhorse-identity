import { render, screen, fireEvent } from '@testing-library/svelte';
import { it, expect, vi, afterEach, beforeEach } from 'vitest';
import Page from '../../../../../src/routes/console/profile/+page.svelte';
vi.mock('$app/state', () => ({
	page: {
		url: new URL('https://localhost/console/profile?user=00000000-0000-0000-0000-000000000002')
	}
}));
vi.mock('$app/environment', () => ({ browser: true }));
const profile = {
	id: '00000000-0000-0000-0000-000000000002',
	revision: '0',
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
it('selects the requested principal and delegates all access checks to Rust', async () => {
	const fetcher = vi
		.fn()
		.mockImplementation(() => Promise.resolve(new Response(JSON.stringify(profile))));
	vi.stubGlobal('fetch', fetcher);
	render(Page);
	await screen.findByText('person@example.com (unverified)');
	expect(fetcher).toHaveBeenCalledWith(
		`/api/profiles/${profile.id}`,
		expect.objectContaining({ credentials: 'same-origin' })
	);
	await fireEvent.click(screen.getByRole('button', { name: 'Change picture' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Remove image' }));
	expect(fetcher).toHaveBeenCalledWith(
		`/api/profiles/${profile.id}/picture`,
		expect.objectContaining({ method: 'DELETE' })
	);
});
