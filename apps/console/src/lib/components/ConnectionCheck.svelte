<script lang="ts">
	import type { Connection, HealthPort } from '$lib/health';
	import { Button } from '$lib/components/ui/button';
	let { check }: { check: HealthPort } = $props();
	let state = $state<Connection | 'idle' | 'pending'>('idle');
	const messages = {
		idle: 'Connection not checked',
		pending: 'Checking connection',
		ready: 'Service reachable',
		unavailable: 'Service unavailable. Try again.'
	};

	async function refresh() {
		state = 'pending';
		state = await check();
	}
</script>

<div class="flex flex-col gap-5 sm:flex-row sm:items-center sm:justify-between">
	<p role="status" class="font-mono text-sm text-muted-foreground">{messages[state]}</p>
	<Button onclick={refresh} disabled={state === 'pending'} class="h-11 px-5 font-mono"
		>Check connection <span aria-hidden="true">↗</span></Button
	>
</div>
