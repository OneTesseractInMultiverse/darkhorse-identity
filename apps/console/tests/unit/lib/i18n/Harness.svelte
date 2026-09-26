<script lang="ts">
	import { untrack } from 'svelte';
	import { provideLocalization } from '../../../../src/lib/i18n/context';
	import type { Localization } from '../../../../src/lib/i18n/state';
	import type { AuthState } from '../../../../src/lib/authentication';
	import LanguageSelector from '../../../../src/lib/components/LanguageSelector.svelte';
	import LoginPanel from '../../../../src/lib/components/LoginPanel.svelte';
	let {
		language,
		remember,
		signIn,
		checkSession
	}: {
		language: Localization;
		remember: () => boolean;
		signIn: () => Promise<AuthState>;
		checkSession: () => Promise<AuthState>;
	} = $props();
	provideLocalization(untrack(() => language));
</script>

<LanguageSelector {remember} />
<LoginPanel {signIn} {checkSession} />
