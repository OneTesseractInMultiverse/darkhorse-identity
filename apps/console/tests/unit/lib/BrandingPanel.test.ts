import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/svelte';
import { it, expect, vi, beforeEach } from 'vitest';
import BrandingPanel from '../../../src/lib/components/BrandingPanel.svelte';
import LoginBrand from '../../../src/lib/components/LoginBrand.svelte';
const settings = {
	revision: '0',
	logo: true,
	background: false,
	storage_enabled: true,
	bucket: 'private-test'
};
beforeEach(() => {
	HTMLDialogElement.prototype.showModal = function () {
		this.open = true;
	};
	HTMLDialogElement.prototype.close = function () {
		this.open = false;
		this.dispatchEvent(new Event('close'));
	};
});
it('uses focused branding dialogs and reloads current authority after a change', async () => {
	const api = {
		settings: vi.fn().mockResolvedValue({ kind: 'ready', value: settings }),
		upload: vi.fn(),
		remove: vi.fn().mockResolvedValue({ kind: 'ready', value: '1' })
	};
	render(BrandingPanel, { api });
	await screen.findByText(/enabled \(private-test\)/);
	await fireEvent.click(screen.getByRole('button', { name: 'Change logo' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Remove image' }));
	await waitFor(() => expect(api.settings).toHaveBeenCalledTimes(2));
	expect(api.remove).toHaveBeenCalledWith('/api/admin/branding/logo', '0');
	api.settings.mockResolvedValue({ kind: 'failed', message: 'Sign in again' });
	await fireEvent.click(screen.getByRole('button', { name: 'Reload settings' }));
	await screen.findByRole('alert');
	expect(screen.queryByText(/private-test/)).not.toBeInTheDocument();
});
it('shows disabled storage and discards settings after leaving', async () => {
	const api = {
		settings: vi.fn().mockResolvedValue({
			kind: 'ready',
			value: { ...settings, logo: false, storage_enabled: false, bucket: null }
		}),
		upload: vi.fn(),
		remove: vi.fn()
	};
	render(BrandingPanel, { api });
	await screen.findByText(/Image storage: disabled/);
	await fireEvent.click(screen.getByRole('button', { name: 'Change background' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
	expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
	cleanup();
	let done!: (v: unknown) => void;
	api.settings.mockReturnValue(
		new Promise((r) => {
			done = r;
		})
	);
	render(BrandingPanel, { api });
	cleanup();
	done({ kind: 'ready', value: settings });
	await Promise.resolve();
	expect(screen.queryByText(/private-test/)).not.toBeInTheDocument();
});
it('uses fixed public branding paths and falls back when images cannot load', async () => {
	const fetcher = vi
		.fn()
		.mockResolvedValue(
			new Response('{"logo":true,"background":true,"url":"https://evil.example"}')
		);
	render(LoginBrand, { fetcher });
	await waitFor(() =>
		expect(screen.getByAltText('DarkHorse')).toHaveAttribute('src', '/api/branding/logo')
	);
	expect(document.querySelector('img[src="https://evil.example"]')).toBeNull();
	const background = document.querySelector('img[src="/api/branding/background"]')!;
	await fireEvent.error(background);
	expect(document.querySelector('.login-background')).toBeNull();
	await fireEvent.error(screen.getByAltText('DarkHorse'));
	expect(screen.getByAltText('DarkHorse')).not.toHaveAttribute('src', '/api/branding/logo');
	cleanup();
	let finish!: (r: Response) => void;
	render(LoginBrand, {
		fetcher: vi.fn().mockReturnValue(
			new Promise((r) => {
				finish = r;
			})
		)
	});
	cleanup();
	finish(new Response('{"logo":true}'));
	await Promise.resolve();
	expect(screen.queryByAltText('DarkHorse')).not.toBeInTheDocument();
});

it('allows keyboard dismissal of a branding change without submitting an upload', async () => {
	const api = {
		settings: vi.fn().mockResolvedValue({ kind: 'ready', value: settings }),
		upload: vi.fn(),
		remove: vi.fn()
	};
	HTMLDialogElement.prototype.close = function () {
		this.open = false;
		this.dispatchEvent(new Event('close'));
	};
	render(BrandingPanel, { api });
	await fireEvent.click(await screen.findByRole('button', { name: 'Change logo' }));
	(screen.getByRole('dialog') as HTMLDialogElement).close();
	await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
	expect(api.upload).not.toHaveBeenCalled();
});
