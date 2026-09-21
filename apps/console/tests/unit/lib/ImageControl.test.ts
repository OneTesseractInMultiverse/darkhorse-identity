import { render, screen, fireEvent, cleanup } from '@testing-library/svelte';
import { it, expect, vi } from 'vitest';
import ImageControl from '../../../src/lib/components/ImageControl.svelte';
function api() {
	return {
		settings: vi.fn(),
		upload: vi.fn().mockResolvedValue({ kind: 'ready', value: '1' }),
		remove: vi.fn().mockResolvedValue({ kind: 'ready', value: '1' })
	};
}
it('selects valid files, records pending state, and publishes only a confirmed change', async () => {
	const service = api(),
		changed = vi.fn(),
		busy = vi.fn();
	render(ImageControl, {
		cancel: vi.fn(),
		api: service,
		path: '/api/profiles/me/picture',
		revision: '0',
		changed,
		busy
	});
	expect(screen.getByRole('button', { name: 'Upload image' })).toBeDisabled();
	await fireEvent.change(screen.getByLabelText('Choose image'), {
		target: { files: [new File(['<svg/>'], 'bad.svg', { type: 'image/svg+xml' })] }
	});
	expect(screen.getByRole('alert')).toHaveTextContent('PNG or JPEG');
	const file = new File(['png'], 'avatar.png', { type: 'image/png' });
	await fireEvent.change(screen.getByLabelText('Choose image'), { target: { files: [file] } });
	await fireEvent.click(screen.getByRole('button', { name: 'Upload image' }));
	expect(service.upload).toHaveBeenCalledWith('/api/profiles/me/picture', '0', file);
	expect(changed).toHaveBeenCalledOnce();
	expect(busy.mock.calls).toEqual([[true], [false]]);
});
it('blocks another mutation after failure and discards a late result after unmount', async () => {
	const service = api();
	service.remove.mockResolvedValue({ kind: 'failed', message: 'Reload required' });
	render(ImageControl, {
		cancel: vi.fn(),
		api: service,
		path: 'x',
		revision: '0',
		changed: vi.fn(),
		busy: vi.fn()
	});
	await fireEvent.click(screen.getByRole('button', { name: 'Remove image' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('Reload required');
	expect(screen.getByRole('button', { name: 'Remove image' })).toBeDisabled();
	cleanup();
	let finish!: (v: unknown) => void;
	const second = api(),
		changed = vi.fn();
	second.remove.mockReturnValue(
		new Promise((r) => {
			finish = r;
		})
	);
	render(ImageControl, {
		cancel: vi.fn(),
		api: second,
		path: 'x',
		revision: '0',
		changed,
		busy: vi.fn()
	});
	await fireEvent.click(screen.getByRole('button', { name: 'Remove image' }));
	cleanup();
	finish({ kind: 'ready', value: '1' });
	await Promise.resolve();
	expect(changed).not.toHaveBeenCalled();
});
