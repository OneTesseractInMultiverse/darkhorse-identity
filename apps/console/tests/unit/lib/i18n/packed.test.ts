import { it, expect } from 'vitest';
import { packCatalog } from '../../../../src/lib/i18n/catalog';
import { createFormatter } from '../../../../src/lib/i18n/format';

const schema = { plain: {}, count: { count: 'number' }, person: { name: 'string' } } as const;
const en = [
	['person', 'Hello {name}'],
	['plain', 'Ready'],
	['count', '{count, plural, one {# item} other {# items}}']
];
const es = [
	['plain', 'Listo'],
	['count', '{count, plural, one {# elemento} other {# elementos}}'],
	['person', 'Hola {name}']
];
it('packs only validated source entries and preserves keys, plural behavior and literal values regardless of source order', () => {
	const packed = { en: packCatalog(schema, en, 'en'), es: packCatalog(schema, es, 'es') };
	expect(packed.en[0]).toEqual([2, 'Hello {name}']);
	const original = createFormatter(schema, { en, es });
	const format = createFormatter(schema, packed);
	for (const locale of ['en', 'es'] as const) {
		expect(format(locale, 'plain')).toEqual(original(locale, 'plain'));
		for (const count of [0, 1, 2, 1000])
			expect(format(locale, 'count', { count })).toEqual(original(locale, 'count', { count }));
		expect(format(locale, 'person', { name: 'María {literal} <x>' })).toEqual(
			original(locale, 'person', { name: 'María {literal} <x>' })
		);
	}
	expect(() => packCatalog(schema, [...en, en[0]], 'en')).toThrow();
	expect(() => packCatalog(schema, [['plain', '<script>'], ...en.slice(1)], 'en')).toThrow();
});
it('rejects invalid packed indices, duplicates, oversized catalogs and malformed entries before rendering', () => {
	const packed = packCatalog(schema, en, 'en');
	for (const value of [-1, 3, 0.5, NaN, Infinity, Number.MAX_SAFE_INTEGER]) {
		expect(() => createFormatter(schema, { en: [[value, 'Ready'], ...packed.slice(1)] })).toThrow();
	}
	for (const value of [
		[packed[0], packed[0], packed[2]],
		Array(513).fill([0, 'Ready']),
		[null],
		[[0]],
		5
	]) {
		expect(() => createFormatter(schema, { en: value })).toThrow();
	}
	expect(createFormatter(schema, { en, es: [[0, 'Listo']] })('es', 'plain')).toEqual({
		locale: 'en',
		text: 'Ready'
	});
});
