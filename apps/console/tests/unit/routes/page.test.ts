import { fireEvent, render, screen } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import Page from '../../../src/routes/+page.svelte';
import { checkHealth } from '../../../src/lib/health';

vi.mock('../../../src/lib/health', () => ({ checkHealth: vi.fn().mockResolvedValue('ready') }));

it('connects the preview page to the health boundary', async () => {
	render(Page);
	expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Access begins');
	expect(screen.getByRole('link', { name: 'Darkhorse home' })).toHaveAttribute('href', '/');
	await fireEvent.click(screen.getByRole('button', { name: 'Check connection' }));
	expect(await screen.findByText('Service reachable')).toBeInTheDocument();
	expect(checkHealth).toHaveBeenCalledTimes(1);
});
