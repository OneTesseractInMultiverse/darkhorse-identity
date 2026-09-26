<script lang="ts">
	import { useLocalization } from '$lib/i18n/context';
	const language = useLocalization();
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
	let alive = false;
	let clearHint = () => {};
	onMount(() => {
		alive = true;
		void refresh();
		return () => {
			alive = false;
			clearHint();
		};
	});
	function accept(next: AuthorizationState) {
		clearHint();
		clearHint = next.kind === 'pending' ? language.hint(next.ui_locale ?? undefined) : () => {};
		flow = next;
		if (next.kind === 'redirect') navigate(next.url);
	}
	async function refresh() {
		busy = true;
		const result = await load();
		if (!alive) return;
		accept(result);
		busy = false;
	}
	async function choose(choice: 'approve' | 'deny') {
		if (busy || flow.kind !== 'pending') return;
		busy = true;
		const result = await decide(flow.request_id, choice);
		if (!alive) return;
		accept(result);
		busy = false;
	}
	async function authenticate(email: string, password: string) {
		const result = await signIn(email, password);
		if (!alive) return result;
		if (result.kind === 'signed-in') {
			language.account(result.locale);
			await refresh();
		}
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
		<div class="panel-top"><span>{$language.t('authorization.connection')}</span></div>
		<div class="panel-body">
			{#if busy}<h1 id="authorization-title" class="break-words">
					{$language.t('authorization.connecting')}
				</h1>
			{:else if flow.kind === 'pending' && flow.status === 'consent'}
				<h1 id="authorization-title" class="break-words">
					{$language.t('authorization.connect', { name: flow.client_name })}
				</h1>
				<p class="intro">{$language.t('authorization.requesting')}</p>
				<ul class="my-5 space-y-2">
					{#each flow.scopes as scope (scope)}<li>
							<code>{scope}</code>{#if scope === 'openid'}<span class="ml-2"
									>{$language.t('authorization.identity')}</span
								>{/if}
						</li>{/each}
				</ul>
				{#if flow.resource}<p class="caption break-all">
						{$language.t('authorization.resource', { resource: flow.resource })}
					</p>{/if}
				<Button class="mt-5 h-11 w-full" onclick={() => choose('approve')}
					>{$language.t('authorization.allow')}</Button
				>
				<Button variant="outline" class="mt-3 h-11 w-full" onclick={() => choose('deny')}
					>{$language.t('common.cancel')}</Button
				>
			{:else}
				<h1 id="authorization-title" class="break-words">
					{$language.t('authorization.unavailable')}
				</h1>
				<p role="status" class="intro">
					{$language.t('authorization.help')}
				</p>
				{#if flow.kind === 'pending'}<Button
						variant="outline"
						class="mt-5 h-11 w-full"
						onclick={() => choose('deny')}>{$language.t('authorization.cancel')}</Button
					>{/if}
			{/if}
		</div>
	</section>
{/if}
