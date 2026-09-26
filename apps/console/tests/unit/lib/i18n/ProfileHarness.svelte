<script lang="ts">
	import { untrack } from 'svelte';
	import { provideLocalization } from '../../../../src/lib/i18n/context';
	import type { Localization } from '../../../../src/lib/i18n/state';
	import type { ProfileApi } from '../../../../src/lib/profiles';
	import ProfilePanel from '../../../../src/lib/components/ProfilePanel.svelte';
	let { language, api }: { language: Localization; api: ProfileApi } = $props();
	provideLocalization(untrack(() => language));
</script>

<ProfilePanel {api} />
