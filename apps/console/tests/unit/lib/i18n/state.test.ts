import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { expect, it, vi } from 'vitest';
import { createLocalization } from '../../../../src/lib/i18n/state';
import { createFormatter } from '../../../../src/lib/i18n/format';
import { contract } from '../../../../src/lib/i18n/contract';
import Harness from './Harness.svelte';
function fake() {
	const en = Object.keys(contract).map((key) => [
		key,
		key === 'login.welcome'
			? 'Welcome {name}'
			: [
					key,
					...Object.keys(contract[key as keyof typeof contract]).map((name) => `{${name}}`)
				].join(' ')
	]);
	const es = en.map(([key, value]) => [key, `ES ${value}`]);
	return createLocalization(createFormatter(contract, { en, es }));
}
it('keeps each render tree independent and preserves credentials while translating a current error', async () => {
	const first = fake(),
		second = fake();
	const remember = vi.fn().mockReturnValue(false);
	const signIn = vi.fn().mockResolvedValue({ kind: 'signed-out' });
	render(Harness, {
		language: first,
		remember,
		signIn,
		checkSession: async () => ({ kind: 'signed-out' })
	});
	await waitFor(() => expect(screen.getByRole('button', { name: 'login.submit' })).toBeEnabled());
	await fireEvent.input(screen.getByLabelText('login.email'), {
		target: { value: 'ada@example.com' }
	});
	await fireEvent.input(screen.getByLabelText('login.password'), {
		target: { value: 'not-translated' }
	});
	await fireEvent.change(screen.getByRole('combobox'), { target: { value: 'es' } });
	expect(screen.getByRole('combobox', { name: 'ES language.label' })).toHaveValue('es');
	expect(screen.getByLabelText('ES login.email')).toHaveValue('ada@example.com');
	expect(screen.getByLabelText('ES login.password')).toHaveValue('not-translated');
	expect(screen.getByRole('status')).toHaveTextContent('ES language.unsaved');
	expect(remember).toHaveBeenCalledWith('es');
	expect(get(second).locale).toBe('en');
	await fireEvent.submit(screen.getByRole('form', { name: 'ES login.submit' }));
	const alert = await screen.findByRole('alert');
	await waitFor(() => expect(alert).toHaveFocus());
	expect(alert).toHaveTextContent('ES login.invalid');
	expect(screen.getByLabelText('ES login.password')).toHaveValue('');
	await fireEvent.change(screen.getByRole('combobox'), { target: { value: 'en' } });
	expect(alert).toHaveTextContent('login.invalid');
	expect(signIn).toHaveBeenCalledWith('ada@example.com', 'not-translated');
});
it('renders interpolated profile content as text rather than markup', async () => {
	const language = fake();
	language.select('es');
	const name = '<img src=x onerror=alert(1)>';
	const rendered = render(Harness, {
		language,
		remember: () => true,
		signIn: async () => ({ kind: 'signed-out' }),
		checkSession: async () => ({ kind: 'signed-in', name })
	});
	expect(await screen.findByRole('heading', { name: `ES Welcome ${name}` })).toBeInTheDocument();
	expect(rendered.container.querySelector('img')).toBeNull();
});
it('uses the rendered English fallback when the optional Spanish catalog is unavailable', () => {
	const en = Object.keys(contract).map((key) => [
		key,
		key === 'login.welcome'
			? 'Welcome {name}'
			: [
					key,
					...Object.keys(contract[key as keyof typeof contract]).map((name) => `{${name}}`)
				].join(' ')
	]);
	const language = createLocalization(createFormatter(contract, { en }));
	language.select('es');
	expect(get(language).locale).toBe('en');
});
it('isolates saved account choice from anonymous hints and clears it on sign-out', () => {
	const language = fake();
	language.initialize({ anonymous: 'en', browser: ['es'] });
	language.account('es');
	expect(get(language).locale).toBe('es');
	language.account(undefined);
	expect(get(language).locale).toBe('en');
	language.account('es');
	language.select('en');
	language.initialize({ anonymous: 'es', deployment: 'es' });
	expect(get(language).locale).toBe('en');
	language.account('es');
	expect(get(language).locale).toBe('en');
});
it('ignores session restoration after leaving a render tree', async () => {
	const language = fake();
	let finish!: (state: { kind: 'signed-in'; name: string; locale: 'es' }) => void;
	const pending = new Promise<{ kind: 'signed-in'; name: string; locale: 'es' }>((resolve) => {
		finish = resolve;
	});
	const view = render(Harness, {
		language,
		remember: () => true,
		signIn: async () => ({ kind: 'signed-out' }),
		checkSession: () => pending
	});
	view.unmount();
	finish({ kind: 'signed-in', name: 'Previous account', locale: 'es' });
	await pending;
	expect(get(language).locale).toBe('en');
});
it('does not allow an older session preference read to replace a confirmed profile change', async () => {
	const language = fake();
	let finish!: (locale: 'es') => void;
	const pending = language.restoreAccount(
		() =>
			new Promise<'es'>((resolve) => {
				finish = resolve;
			})
	);
	language.account('en');
	finish('es');
	await pending;
	expect(get(language).locale).toBe('en');
});
