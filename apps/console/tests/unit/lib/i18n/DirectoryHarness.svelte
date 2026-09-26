<script lang="ts">
	import { untrack } from 'svelte';
	import { provideLocalization } from '../../../../src/lib/i18n/context';
	import type { Localization } from '../../../../src/lib/i18n/state';
	import type { DirectoryApi } from '../../../../src/lib/admin/directory';
	import DirectoryPanel from '../../../../src/lib/components/admin/DirectoryPanel.svelte';
	let { language, api }: { language: Localization; api: DirectoryApi } = $props();
	provideLocalization(untrack(() => language));
</script>

<DirectoryPanel {api} />
