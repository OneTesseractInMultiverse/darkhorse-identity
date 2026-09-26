import { expect, it } from 'vitest';
import { compileCatalog } from '../../../../src/lib/i18n/catalog';
import { createFormatter } from '../../../../src/lib/i18n/format';
const contract = { hello: { name: 'string' }, items: { count: 'number' }, plain: {} } as const;
const en = [
	['hello', 'Hello, {name}.'],
	['items', '{count, plural, =0 {Empty} one {# item} other {# items}}'],
	['plain', 'Ready']
];
const es = [
	['hello', 'Hola, {name}.'],
	['items', '{count, plural, =0 {Vacío} one {# elemento} other {# elementos}}'],
	['plain', 'Listo']
];

it('formats trusted ICU messages and uses English for absent or broken catalogs', () => {
	const t = createFormatter(contract, { en, es });
	expect(t('es', 'hello', { name: '<img src=x onerror=alert(1)>' })).toEqual({
		locale: 'es',
		text: 'Hola, <img src=x onerror=alert(1)>.'
	});
	for (const [count, text] of [
		[0, 'Vacío'],
		[1, '1 elemento'],
		[2, '2 elementos']
	] as const)
		expect(t('es', 'items', { count }).text).toBe(text);
	expect(createFormatter(contract, { en })('es', 'plain')).toEqual({ locale: 'en', text: 'Ready' });
	expect(createFormatter(contract, { en, es: [['plain', '{broken']] })('es', 'plain').text).toBe(
		'Ready'
	);
	expect(() => createFormatter(contract, { en: [] })).toThrow('Invalid message catalog');
});
it('rejects structural defects, unsafe source content and mismatched argument contracts', () => {
	const invalid = [
		[],
		[...en, en[0]],
		[...en, ['unknown', 'No']],
		[['hello', 'Hello'], ...en.slice(1)],
		[['hello', '{wrong}'], ...en.slice(1)],
		[['hello', '<b>{name}</b>'], ...en.slice(1)],
		[['hello', '{name, number}'], ...en.slice(1)],
		[en[0], ['items', '{count, plural, one {One}}'], en[2]],
		[en[0], ['items', '{count, plural, other {#} bogus {No}}'], en[2]],
		[en[0], ['items', '{count, date}'], en[2]],
		[en[0], ['items', '{count, plural, offset:1 other {#}}'], en[2]],
		[en[0], ['items', '{count, number, ::currency/USD}'], en[2]],
		[['hello', 'x'.repeat(2049)], ...en.slice(1)]
	];
	for (const catalog of invalid)
		expect(() => compileCatalog(contract, catalog, 'en')).toThrow('Invalid message catalog');
});
it('enforces interpolation types and bounds without exposing values in failures', () => {
	const t = createFormatter(contract, { en, es });
	expect(() => t('en', 'hello', { name: 'x'.repeat(4097) })).toThrow('Invalid message arguments');
	expect(() => t('en', 'items', { count: Infinity })).toThrow('Invalid message arguments');
	// Compile-time checks intentionally live in checked test source; they do not execute.
	function invalidCalls() {
		// @ts-expect-error unknown message key
		t('en', 'secret');
		// @ts-expect-error missing required interpolation
		t('en', 'hello');
		// @ts-expect-error wrong plural type
		t('es', 'items', { count: '2' });
		// @ts-expect-error extra argument on literal message
		t('es', 'plain', { name: 'Ada' });
	}
	void invalidCalls;
});
it('supports plain numeric formatting, ordinal categories and select without custom styles', () => {
	const c = {
		number: { count: 'number' },
		choice: { mode: 'string' },
		ordinal: { count: 'number' }
	} as const;
	const t = createFormatter(c, {
		en: [
			['number', '{count, number}'],
			['choice', '{mode, select, short {Yes} other {No}}'],
			['ordinal', '{count, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}']
		]
	});
	expect(t('en', 'number', { count: 1234 }).text).toBe('1,234');
	expect(t('en', 'choice', { mode: 'unknown' }).text).toBe('No');
	expect(t('en', 'choice', { mode: 'short' }).text).toBe('Yes');
	expect(t('en', 'ordinal', { count: 2 }).text).toBe('2nd');
	expect(() => Reflect.apply(t, undefined, ['en', 'absent'])).toThrow('Invalid message arguments');
	expect(() =>
		Reflect.apply(t, undefined, ['en', 'number', { count: 'credential-like-value' }])
	).toThrow('Invalid message arguments');
});
it('bounds the entire parsed message across sibling branches', () => {
	const c = { choice: { name: 'string' } } as const;
	const branch = (count: number) => '{name}'.repeat(count);
	const message = (left: number, right: number) =>
		`{name, select, a {${branch(left)}} other {${branch(right)}}}`;
	const accepted = createFormatter(c, { en: [['choice', message(127, 128)]] });
	expect(accepted('en', 'choice', { name: 'a' }).text).toBe('a'.repeat(127));
	expect(accepted('en', 'choice', { name: 'z' }).text).toBe('z'.repeat(128));
	expect(() => compileCatalog(c, [['choice', message(128, 128)]], 'en')).toThrow(
		'Invalid message catalog'
	);
});
it('rejects invisible directional controls in trusted catalog text', () => {
	for (const control of ['\u061c', '\u200e', '\u200f', '\u202a', '\u202e', '\u2066', '\u2069'])
		expect(() => compileCatalog({ plain: {} }, [['plain', `Safe${control}text`]], 'en')).toThrow(
			'Invalid message catalog'
		);
});
it('rejects malformed entries, invalid select arguments and excessive nesting', () => {
	for (const entry of [null, ['hello'], ['hello', 'ok', 'extra'], [1, 'text'], ['hello', 1]])
		expect(() => compileCatalog({ hello: {} }, [entry], 'en')).toThrow('Invalid message catalog');
	expect(() =>
		compileCatalog(
			{ choice: { count: 'number' } },
			[['choice', '{count, select, other {Any}}']],
			'en'
		)
	).toThrow('Invalid message catalog');
	let nested = 'value';
	for (let depth = 0; depth < 9; depth++) nested = `{name, select, other {${nested}}}`;
	expect(() => compileCatalog({ choice: { name: 'string' } }, [['choice', nested]], 'en')).toThrow(
		'Invalid message catalog'
	);
});
