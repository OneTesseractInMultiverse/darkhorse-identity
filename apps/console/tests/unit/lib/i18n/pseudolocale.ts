import { TYPE, type MessageFormatElement } from '@formatjs/icu-messageformat-parser';
import { compileCatalog, type Contract } from '../../../../src/lib/i18n/catalog';
import type { Locale } from '../../../../src/lib/i18n/locale';

/** Test-only pseudo-locale. It is outside src/ and is never imported by production code. */
export function pseudolocalizeCatalog(
	contract: Contract,
	value: unknown,
	locale: Locale
): [string, string][] {
	const validated = compileCatalog(contract, value, locale);
	const pseudo = [...validated].map(([key, message]) => [key, render(message)] as [string, string]);
	compileCatalog(contract, pseudo, locale);
	return pseudo;
}

function render(message: MessageFormatElement[]): string {
	return message.map(renderElement).join('');
}

function renderElement(element: MessageFormatElement): string {
	if (element.type === TYPE.literal) return literal(element.value);
	if (element.type === TYPE.pound) return '#';
	if (element.type === TYPE.argument) return `{${element.value}}`;
	if (element.type === TYPE.number) return `{${element.value}, number}`;
	if (element.type === TYPE.select || element.type === TYPE.plural) {
		const format =
			element.type === TYPE.select
				? 'select'
				: element.pluralType === 'ordinal'
					? 'selectordinal'
					: 'plural';
		const options = Object.entries(element.options)
			.map(([key, branch]) => `${key} {${render(branch.value)}}`)
			.join(' ');
		return `{${element.value}, ${format}, ${options}}`;
	}
	throw new Error('Unsupported pseudo-locale message.');
}

function literal(value: string): string {
	const accented = [...value]
		.map(
			(character) =>
				({ a: 'á', e: 'é', i: 'í', o: 'ó', u: 'ú', A: 'Á', E: 'É', I: 'Í', O: 'Ó', U: 'Ú' })[
					character
				] ?? character
		)
		.join('');
	const letters = [...value].filter((character) => /[a-z]/i.test(character)).length;
	const expansion = '~'.repeat(Math.min(64, Math.ceil(letters / 4)));
	const escaped = accented.replaceAll("'", "''").replaceAll('{', "'{'").replaceAll('}', "'}'");
	return `⟦${escaped}${expansion}⟧`;
}
