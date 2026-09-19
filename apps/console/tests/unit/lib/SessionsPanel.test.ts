import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { expect, it, vi, beforeEach } from 'vitest';
import SessionsPanel from '../../../src/lib/components/SessionsPanel.svelte';
const id = '00000000-0000-0000-0000-000000000001';
const other = '00000000-0000-0000-0000-000000000002';
const row = {
	id: other,
	created_ms: 1000,
	seen_ms: 2000,
	expires_ms: 3000,
	status: 'active' as const
};
function props() {
	return {
		read: vi.fn().mockResolvedValue({
			kind: 'ready',
			page: { current: id, items: [row], next: `1000:${other}` }
		}),
		end: vi.fn().mockResolvedValue({ kind: 'ended', current: false })
	};
}
beforeEach(() => {
	HTMLDialogElement.prototype.showModal = function () {
		this.open = true;
	};
	HTMLDialogElement.prototype.close = function () {
		this.open = false;
		this.dispatchEvent(new Event('close'));
	};
});
it('loads a bounded table, confirms ending a session, and reloads after success', async () => {
	const p = props();
	render(SessionsPanel, p);
	expect(await screen.findByRole('cell', { name: '1970-01-01 00:00:02 UTC' })).toBeInTheDocument();
	await fireEvent.click(screen.getByRole('button', { name: 'End session' }));
	expect(p.end).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
	expect(p.end).not.toHaveBeenCalled();
	await fireEvent.click(screen.getByRole('button', { name: 'End session' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm end session' }));
	await waitFor(() => expect(p.end).toHaveBeenCalledExactlyOnceWith(other));
	await waitFor(() => expect(p.read).toHaveBeenCalledTimes(2));
	await fireEvent.click(screen.getByRole('button', { name: 'Older sessions' }));
	await waitFor(() => expect(p.read).toHaveBeenLastCalledWith(`1000:${other}`));
	await fireEvent.click(screen.getByRole('button', { name: 'Newest sessions' }));
	await waitFor(() => expect(p.read).toHaveBeenLastCalledWith(undefined));
});
it('disables repeat mutations after an uncertain response until a successful refresh', async () => {
	const p = props();
	p.end.mockResolvedValue({ kind: 'uncertain' });
	render(SessionsPanel, p);
	await fireEvent.click(await screen.findByRole('button', { name: 'End session' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm end session' }));
	expect(await screen.findByRole('alert')).toHaveTextContent('could not be confirmed');
	expect(screen.getByRole('button', { name: 'End session' })).toBeDisabled();
	await fireEvent.click(screen.getByRole('button', { name: 'Refresh sessions' }));
	await waitFor(() => expect(screen.getByRole('button', { name: 'End session' })).toBeEnabled());
	expect(p.end).toHaveBeenCalledTimes(1);
});
it('clears session details after ending the current session or losing authentication', async () => {
	const p = props();
	p.read.mockResolvedValue({ kind: 'ready', page: { current: other, items: [row], next: null } });
	p.end.mockResolvedValue({ kind: 'ended', current: true });
	render(SessionsPanel, p);
	await fireEvent.click(await screen.findByRole('button', { name: 'End this session' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm end session' }));
	expect(await screen.findByRole('link', { name: 'Sign in' })).toBeInTheDocument();
	expect(screen.queryByRole('table')).not.toBeInTheDocument();
});
it('shows unavailable and signed-out states without displaying stale rows', async () => {
	const p = props();
	p.read.mockResolvedValueOnce({ kind: 'unavailable' }).mockResolvedValue({ kind: 'signed-out' });
	render(SessionsPanel, p);
	expect(await screen.findByRole('alert')).toHaveTextContent('unavailable');
	await fireEvent.click(screen.getByRole('button', { name: 'Refresh sessions' }));
	expect(await screen.findByRole('link', { name: 'Sign in' })).toBeInTheDocument();
});
it('keeps a submitted confirmation locked until the outcome arrives', async () => {
	const p = props();
	let complete!: (value: { kind: 'ended'; current: boolean }) => void;
	p.end.mockReturnValue(
		new Promise((resolve) => {
			complete = resolve;
		})
	);
	render(SessionsPanel, p);
	await fireEvent.click(await screen.findByRole('button', { name: 'End session' }));
	await fireEvent.click(screen.getByRole('button', { name: 'Confirm end session' }));
	const confirm = screen.getByRole('button', { name: 'Ending session…' });
	expect(confirm).toBeDisabled();
	expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
	await fireEvent.click(confirm);
	const event = new Event('cancel', { cancelable: true });
	screen.getByRole('dialog').dispatchEvent(event);
	expect(event.defaultPrevented).toBe(true);
	expect(p.end).toHaveBeenCalledTimes(1);
	complete({ kind: 'ended', current: false });
	await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
});
it('renders inactive history and empty older pages without offering mutations', async () => {
	const p = props();
	p.read
		.mockResolvedValueOnce({
			kind: 'ready',
			page: { current: id, items: [{ ...row, status: 'inactive' }], next: `1000:${other}` }
		})
		.mockResolvedValue({ kind: 'ready', page: { current: id, items: [], next: null } });
	render(SessionsPanel, p);
	expect(await screen.findByText('Ended or expired')).toBeInTheDocument();
	expect(screen.queryByRole('button', { name: 'End session' })).not.toBeInTheDocument();
	await fireEvent.click(screen.getByRole('button', { name: 'Older sessions' }));
	expect(await screen.findByText('No sessions on this page.')).toBeInTheDocument();
});
