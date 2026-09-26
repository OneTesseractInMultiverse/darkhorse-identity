<script lang="ts">
	import { untrack, type ComponentProps } from 'svelte';
	import { provideLocalization } from '../../../../src/lib/i18n/context';
	import type { Localization } from '../../../../src/lib/i18n/state';
	import KeyEditor from '../../../../src/lib/components/KeyEditor.svelte';
	let {
		language,
		properties
	}: { language: Localization; properties: ComponentProps<typeof KeyEditor> } = $props();
	provideLocalization(untrack(() => language));
</script>

<KeyEditor {...properties} />
