import { expect, it } from 'vitest';
import { get, writable } from 'svelte/store';
import { createFormatter } from '../../../../src/lib/i18n/format';
import { scopedMessages } from '../../../../src/lib/i18n/scoped';
import type { Locale } from '../../../../src/lib/i18n/locale';

it('keeps a route formatter isolated and reports its actual fallback language', () => {
	const contract = { label: {}, name: { value: 'string' } } as const;
	const en = [
		['label', 'Label'],
		['name', 'Name: {value}']
	];
	const es = [
		['label', 'Etiqueta'],
		['name', 'Nombre: {value}']
	];
	const first = writable<{ locale: Locale }>({ locale: 'es' });
	const second = writable<{ locale: Locale }>({ locale: 'en' });
	const format = createFormatter(contract, { en, es });
	const a = scopedMessages(first, format, (locale) => format(locale, 'label').locale);
	const b = scopedMessages(second, format, (locale) => format(locale, 'label').locale);
	expect(get(a).t('name', { value: '<literal> {token}' })).toBe('Nombre: <literal> {token}');
	expect(get(b).t('label')).toBe('Label');
	first.set({ locale: 'en' });
	expect(get(a).t('label')).toBe('Label');
	expect(get(b).locale).toBe('en');
	const incomplete = createFormatter(contract, { en, es: [] });
	const fallback = scopedMessages(
		writable({ locale: 'es' as const }),
		incomplete,
		(locale) => incomplete(locale, 'label').locale
	);
	expect(get(fallback).locale).toBe('en');
	expect(get(fallback).t('label')).toBe('Label');
});
