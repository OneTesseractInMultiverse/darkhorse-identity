import type { Locale } from './locale';
/** A bounded language-tag adapter. Never accepts a path, header or import specifier. */
export function localeTag(value: unknown): Locale | undefined {
	if (
		typeof value !== 'string' ||
		value.length > 64 ||
		!/^[a-zA-Z]{2,8}(?:-[a-zA-Z0-9]{1,8})*$/.test(value)
	)
		return;
	try {
		const primary = Intl.getCanonicalLocales(value)[0].split('-')[0];
		return primary === 'en' || primary === 'es' ? primary : undefined;
	} catch {
		return;
	}
}
export function localeList(values: unknown): Locale[] {
	if (!Array.isArray(values) || values.length > 16) return [];
	return [...new Set(values.map(localeTag).filter((v): v is Locale => v !== undefined))];
}
