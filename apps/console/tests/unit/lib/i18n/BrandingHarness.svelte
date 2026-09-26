<script lang="ts">
	import { untrack } from 'svelte';
	import { provideLocalization } from '../../../../src/lib/i18n/context';
	import type { Localization } from '../../../../src/lib/i18n/state';
	import type { MediaApi } from '../../../../src/lib/media';
	import BrandingPanel from '../../../../src/lib/components/BrandingPanel.svelte';
	import ConsoleShell from '../../../../src/lib/components/admin/ConsoleShell.svelte';
	let { language, api }: { language: Localization; api: MediaApi } = $props();
	provideLocalization(untrack(() => language));
</script>

<ConsoleShell active="settings"><BrandingPanel {api} /></ConsoleShell>
