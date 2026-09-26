import { derived, type Readable } from 'svelte/store';
import type { Contract } from './catalog';
import type { Formatter, Arguments } from './format';
import type { Locale } from './locale';

/** Route-specific messages follow the layout's choice without owning account state. */
export function scopedMessages<C extends Contract>(
	source: Readable<{ locale: Locale }>,
	format: Formatter<C>,
	resolved: (requested: Locale) => Locale
) {
	return derived(source, (state) => {
		const locale = resolved(state.locale);
		function t<K extends keyof C & string>(
			key: K,
			...args: keyof C[K] extends never ? [] : [Arguments<C, K>]
		) {
			return format(locale, key, ...args).text;
		}
		return { locale, t };
	});
}
