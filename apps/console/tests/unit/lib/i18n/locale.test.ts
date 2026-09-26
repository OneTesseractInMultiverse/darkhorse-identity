import { expect, it } from 'vitest';
import { localeTag, localeList } from '../../../../src/lib/i18n/input';
import { resolveLocale } from '../../../../src/lib/i18n/locale';

it('matches bounded canonical and regional tags without interpreting paths or headers', () => {
	for (const tag of ['es', 'ES', 'es-CR', 'es-419', 'es-Latn-ES'])
		expect(localeTag(tag)).toBe('es');
	for (const tag of ['en', 'en-US', 'en-GB']) expect(localeTag(tag)).toBe('en');
	for (const tag of [
		undefined,
		null,
		{},
		'',
		'fr',
		'../es',
		'es_ES',
		'es;q=1',
		' es ',
		'es--CR',
		'es-'.repeat(40),
		'<script>',
		'es-CR-CR'
	])
		expect(localeTag(tag)).toBeUndefined();
	expect(localeList(['fr', 'es-CR', 'en', 'es'])).toEqual(['es', 'en']);
	expect(localeList(Array(17).fill('es'))).toEqual([]);
	expect(localeList('es')).toEqual([]);
});
it('resolves nonauthoritative presentation preferences in a fixed order', () => {
	const all = {
		explicit: 'es',
		saved: 'en',
		anonymous: 'en',
		transaction: ['en'],
		browser: ['en'],
		deployment: 'en'
	} as const;
	expect(resolveLocale(all)).toBe('es');
	expect(resolveLocale({ ...all, explicit: undefined, saved: 'es' })).toBe('es');
	expect(resolveLocale({ anonymous: 'es', transaction: ['en'] })).toBe('es');
	expect(resolveLocale({ transaction: ['es', 'en'], browser: ['en'] })).toBe('es');
	expect(resolveLocale({ browser: ['es', 'en'], deployment: 'en' })).toBe('es');
	expect(resolveLocale({ deployment: 'es' })).toBe('es');
	expect(resolveLocale({})).toBe('en');
});
