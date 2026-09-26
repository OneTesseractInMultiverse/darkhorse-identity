import { getContext, setContext } from 'svelte';
import { useLocalization } from './context';
import { scopedMessages } from './scoped';
import { createFormatter, type Formatter } from './format';
import { adminContract } from './admin-contract';
import en from './catalogs/admin-en.json';
import es from './catalogs/admin-es.json';

const key = Symbol('administration-messages');
export function provideCatalogLocalization(
	format: Formatter<typeof adminContract> = createFormatter(adminContract, { en, es })
) {
	return setContext(
		key,
		scopedMessages(useLocalization(), format, (locale) => format(locale, 'picker.loading').locale)
	);
}
export function useCatalogLocalization(): ReturnType<typeof provideCatalogLocalization> {
	return (
		getContext<ReturnType<typeof provideCatalogLocalization> | undefined>(key) ??
		provideCatalogLocalization()
	);
}
