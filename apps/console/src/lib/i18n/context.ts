import { getContext, setContext } from 'svelte';
import { createFormatter } from './format';
import { contract } from './contract';
import en from './catalogs/en.json';
import es from './catalogs/es.json';
import { createLocalization, type Localization } from './state';
// Immutable key only. Each layout/component tree owns its mutable store and formatters.
const key = Symbol('localization');
function create() {
	return createLocalization(createFormatter(contract, { en, es }));
}
export function provideLocalization(localization = create()) {
	return setContext(key, localization);
}
export function useLocalization(): Localization {
	return getContext<Localization | undefined>(key) ?? provideLocalization();
}
