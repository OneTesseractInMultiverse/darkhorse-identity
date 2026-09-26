import { writable } from 'svelte/store';
import type { contract } from './contract';
import type { Formatter, Arguments } from './format';
import type { Locale } from './locale';
type Key = keyof typeof contract;
export type Translate = <K extends Key>(
	key: K,
	...args: keyof (typeof contract)[K] extends never ? [] : [Arguments<typeof contract, K>]
) => string;
export type Localization = ReturnType<typeof createLocalization>;
export function createLocalization(format: Formatter<typeof contract>) {
	function view(requested: Locale) {
		const locale = format(requested, 'language.label').locale;
		const t: Translate = (key, ...args) => format(locale, key, ...args).text;
		return { locale, t };
	}
	const state = writable(view('en'));
	return { subscribe: state.subscribe, select: (locale: Locale) => state.set(view(locale)) };
}
