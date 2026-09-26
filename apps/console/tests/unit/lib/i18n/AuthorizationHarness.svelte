<script lang="ts">
	import { untrack } from 'svelte';
	import { provideLocalization } from '../../../../src/lib/i18n/context';
	import type { Localization } from '../../../../src/lib/i18n/state';
	import type { ComponentProps } from 'svelte';
	import AuthorizationPanel from '../../../../src/lib/components/AuthorizationPanel.svelte';
	let {
		language,
		properties
	}: { language: Localization; properties: ComponentProps<typeof AuthorizationPanel> } = $props();
	provideLocalization(untrack(() => language));
</script>

<AuthorizationPanel {...properties} />
