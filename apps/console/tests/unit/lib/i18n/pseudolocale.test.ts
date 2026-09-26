import { expect, it } from 'vitest';
import { createFormatter } from '../../../../src/lib/i18n/format';
import { pseudolocalizeCatalog } from './pseudolocale';

const contract = {
	hello: { name: 'string' },
	items: { count: 'number' },
	quoted: {}
} as const;
const english = [
	['hello', 'Hello, {name}.'],
	['items', '{count, plural, =0 {No files} one {# file} other {# files}}'],
	['quoted', "Use '{' braces '}' safely."]
];

it('expands and accents catalog literals while keeping ICU arguments and plural behavior intact', () => {
	const pseudo = pseudolocalizeCatalog(contract, english, 'en');
	const translate = createFormatter(contract, { en: pseudo });
	const greeting = translate('en', 'hello', { name: 'Ada' }).text;
	const untrusted = '<img src=x onerror=alert(1)>';

	expect(greeting).toContain('Ada');
	expect(greeting).toContain('Hélló');
	expect(greeting.length).toBeGreaterThan('Hello, Ada.'.length);
	expect(translate('en', 'hello', { name: untrusted }).text).toContain(untrusted);
	expect(translate('en', 'items', { count: 2 }).text).toContain('2⟦ fílés');
	expect(translate('en', 'items', { count: 1 }).text).toContain('1⟦ fílé');
	expect(translate('en', 'items', { count: 0 }).text).toContain('⟦Nó fílés');
	expect(translate('en', 'quoted').text).toContain('{');
	expect(translate('en', 'quoted').text).toContain('}');
});

it('uses the same catalog bounds and validation as production translation data', () => {
	expect(() => pseudolocalizeCatalog(contract, [], 'en')).toThrow('Invalid message catalog');
	expect(() =>
		pseudolocalizeCatalog(contract, [['hello', '{unknown}'], ...english.slice(1)], 'en')
	).toThrow('Invalid message catalog');
});
