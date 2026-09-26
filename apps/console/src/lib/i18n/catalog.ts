import { parse, TYPE, type MessageFormatElement } from '@formatjs/icu-messageformat-parser';
import type { Locale } from './locale.ts';
export type Contract = Readonly<Record<string, Readonly<Record<string, 'string' | 'number'>>>>;
export type Catalog = Map<string, MessageFormatElement[]>;
function invalid(): never {
	throw new Error('Invalid message catalog.');
}
/** Reviewed ICU parser; source catalogs contain only messages, never executable markup. */
export function compileCatalog(contract: Contract, value: unknown, locale: Locale): Catalog {
	const keys = Object.keys(contract);
	if (keys.length > 512 || !Array.isArray(value) || value.length !== keys.length) invalid();
	const catalog: Catalog = new Map();
	for (const entry of value) {
		if (!Array.isArray(entry) || entry.length !== 2) invalid();
		const [key, text] = entry;
		if (
			typeof key !== 'string' ||
			!Object.hasOwn(contract, key) ||
			catalog.has(key) ||
			typeof text !== 'string' ||
			text.length < 1 ||
			text.length > 2048 ||
			unsafeText(text)
		)
			invalid();
		let ast: MessageFormatElement[];
		try {
			ast = parse(text);
		} catch {
			invalid();
		}
		const names = new Set<string>();
		inspect(ast, contract[key], names, locale, 0);
		if (names.size !== Object.keys(contract[key]).length) invalid();
		catalog.set(key, ast);
	}
	return catalog;
}
function inspect(
	ast: MessageFormatElement[],
	args: Contract[string],
	names: Set<string>,
	locale: Locale,
	depth: number
): void {
	if (depth > 8 || ast.length > 256) invalid();
	for (const node of ast) {
		if (node.type === TYPE.literal || node.type === TYPE.pound) continue;
		if (node.type === TYPE.tag || node.type === TYPE.date || node.type === TYPE.time) invalid();
		if (!Object.hasOwn(args, node.value)) invalid();
		names.add(node.value);
		if (node.type === TYPE.argument) continue;
		if (node.type === TYPE.number) {
			if (args[node.value] !== 'number' || node.style) invalid();
			continue;
		}
		if (node.type === TYPE.select && args[node.value] !== 'string') invalid();
		if (node.type === TYPE.plural) {
			if (args[node.value] !== 'number' || node.offset !== 0) invalid();
			const categories = new Intl.PluralRules(locale, { type: node.pluralType }).resolvedOptions()
				.pluralCategories;
			if (
				Object.keys(node.options).some(
					(key) =>
						!categories.includes(key as Intl.LDMLPluralRule) && !/^=(0|[1-9]\d{0,8})$/.test(key)
				)
			)
				invalid();
		}
		if (!Object.hasOwn(node.options, 'other')) invalid();
		for (const branch of Object.values(node.options))
			inspect(branch.value, args, names, locale, depth + 1);
	}
}

function unsafeText(text: string): boolean {
	return [...text].some((char) => {
		const code = char.charCodeAt(0);
		return (
			code === 60 ||
			code === 62 ||
			code === 127 ||
			(code < 32 && code !== 9 && code !== 10) ||
			(code >= 0x202a && code <= 0x202e) ||
			(code >= 0x2066 && code <= 0x2069)
		);
	});
}
