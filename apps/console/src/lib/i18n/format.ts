import { IntlMessageFormat } from 'intl-messageformat';
import { compileCatalog, expandCatalog, type Contract } from './catalog';
import type { Locale } from './locale';
export type Arguments<C extends Contract, K extends keyof C> = {
	[P in keyof C[K]]: C[K][P] extends 'number' ? number : string;
};
export type Formatter<C extends Contract> = <K extends keyof C & string>(
	locale: Locale,
	key: K,
	...args: keyof C[K] extends never ? [] : [Arguments<C, K>]
) => { locale: Locale; text: string };
export function createFormatter<C extends Contract>(
	contract: C,
	catalogs: { en: unknown; es?: unknown }
): Formatter<C> {
	const en = messages(contract, catalogs.en, 'en');
	let es: ReturnType<typeof messages> | undefined;
	try {
		es = messages(contract, catalogs.es, 'es');
	} catch {
		/* Absent/invalid optional catalog falls back as a whole. */
	}
	return (locale, key, ...args) => {
		const selected = locale === 'es' && es ? es : en;
		const values = args[0] ?? {};
		validateArguments(contract[key], values);
		const message = selected.get(key);
		if (!message) throw new Error('Invalid message key.');
		return { locale: selected === es ? 'es' : 'en', text: String(message.format(values)) };
	};
}
function messages(contract: Contract, value: unknown, locale: Locale) {
	return new Map(
		[...compileCatalog(contract, expandCatalog(contract, value), locale)].map(([key, ast]) => [
			key,
			new IntlMessageFormat(ast, locale, undefined, { ignoreTag: true })
		])
	);
}
function validateArguments(
	contract: Contract[string] | undefined,
	values: Record<string, unknown>
) {
	if (!contract || Object.keys(contract).length !== Object.keys(values).length)
		throw new Error('Invalid message arguments.');
	for (const [key, type] of Object.entries(contract)) {
		const value = values[key];
		if (
			!Object.hasOwn(values, key) ||
			(type === 'number'
				? typeof value !== 'number' ||
					!Number.isFinite(value) ||
					Math.abs(value) > Number.MAX_SAFE_INTEGER
				: typeof value !== 'string' || value.length > 4096)
		)
			throw new Error('Invalid message arguments.');
	}
}
