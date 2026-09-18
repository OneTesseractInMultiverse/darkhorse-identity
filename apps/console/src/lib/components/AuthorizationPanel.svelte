<script lang="ts">
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import LoginPanel from './LoginPanel.svelte';
	import type { AuthorizationState } from '$lib/authorization';
	import type { AuthState } from '$lib/authentication';
	let {
		load,
		decide,
		signIn,
		navigate
	}: {
		load: () => Promise<AuthorizationState>;
		decide: (id: string, choice: 'approve' | 'deny') => Promise<AuthorizationState>;
		signIn: (email: string, password: string) => Promise<AuthState>;
		navigate: (url: string) => void;
	} = $props();
	let flow = $state<AuthorizationState>({ kind: 'unavailable' });
	let busy = $state(true);
	onMount(() => {
		void refresh();
	});
	function accept(next: AuthorizationState) {
		flow = next;
		if (next.kind === 'redirect') navigate(next.url);
	}
	async function refresh() {
		busy = true;
		accept(await load());
		busy = false;
	}
	async function choose(choice: 'approve' | 'deny') {
		if (busy || flow.kind !== 'pending') return;
		busy = true;
		accept(await decide(flow.request_id, choice));
		busy = false;
	}
	async function authenticate(email: string, password: string) {
		const result = await signIn(email, password);
		if (result.kind === 'signed-in') await refresh();
		return result;
	}
	async function signedOut(): Promise<AuthState> {
		return { kind: 'signed-out' };
	}
</script>

{#if !busy && flow.kind === 'pending' && flow.status === 'login'}
	<LoginPanel signIn={authenticate} checkSession={signedOut} />
{:else}
	<section class="glass login-panel" aria-labelledby="authorization-title" aria-busy={busy}>
		<div class="panel-top"><span>APPLICATION CONNECTION</span></div>
		<div class="panel-body">
			{#if busy}<h1 id="authorization-title" class="break-words">Connecting…</h1>
			{:else if flow.kind === 'pending' && flow.status === 'consent'}
				<h1 id="authorization-title" class="break-words">Connect {flow.client_name}?</h1>
				<p class="intro">This application is requesting:</p>
				<ul class="my-5 space-y-2">
					{#each flow.scopes as scope (scope)}<li>
							{scope === 'openid' ? 'Your signed-in identity' : scope}
						</li>{/each}
				</ul>
				{#if flow.resource}<p class="caption break-all">Resource: {flow.resource}</p>{/if}
				<Button class="mt-5 h-11 w-full" onclick={() => choose('approve')}>Allow connection</Button>
				<Button variant="outline" class="mt-3 h-11 w-full" onclick={() => choose('deny')}
					>Cancel</Button
				>
			{:else}
				<h1 id="authorization-title" class="break-words">Unable to connect</h1>
				<p role="status" class="intro">
					This connection is unavailable. Return to the application and try again.
				</p>
				{#if flow.kind === 'pending'}<Button
						variant="outline"
						class="mt-5 h-11 w-full"
						onclick={() => choose('deny')}>Cancel connection</Button
					>{/if}
			{/if}
		</div>
	</section>
{/if}
