<script lang="ts">
	import { provideLocalization } from '../../../../src/lib/i18n/context';
	import ApiReferencePanel from '../../../../src/lib/components/admin/ApiReferencePanel.svelte';

	const language = provideLocalization();
	language.select('es');
</script>

<ApiReferencePanel />
