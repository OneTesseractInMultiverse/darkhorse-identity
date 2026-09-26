<script lang="ts">
	import type { Item } from '$lib/admin/catalog';
	let { client }: { client: Item } = $props();
</script>

<div class="catalog-detail">
	<dl class="catalog-facts">
		<div class="catalog-fact-wide">
			<dt>Client ID</dt>
			<dd class="catalog-id">{client.id}</dd>
			<dd class="catalog-help">
				Use this value as <code>client_id</code> in your application’s OIDC configuration. It stays the
				same when you rotate the secret.
			</dd>
		</div>
		<div>
			<dt>Application ID</dt>
			<dd class="catalog-id">{client.application_id}</dd>
			<dd class="catalog-help">The application that owns this registration.</dd>
		</div>
		<div>
			<dt>Status</dt>
			<dd>
				<span class="catalog-status" class:inactive={!client.active}
					>{client.active ? 'Active' : 'Inactive'}</span
				>
			</dd>
			<dd class="catalog-help">
				Both this client and its parent application must be active to authenticate.
			</dd>
		</div>
	</dl>
	<section class="catalog-section">
		<h3>Sign-in configuration</h3>
		<p class="catalog-help">Confidential web client · Authorization Code flow with PKCE (S256).</p>
		<dl class="catalog-settings">
			<div>
				<dt>Client authentication</dt>
				<dd>
					<code>client_secret_basic</code>
					<p class="catalog-help">
						Your backend sends the client ID and secret using HTTP Basic authentication over HTTPS
						when exchanging an authorization code. Keep the secret out of browser code and URLs.
					</p>
				</dd>
			</div>
			<div>
				<dt>Callback URLs</dt>
				<dd>
					<ul class="catalog-values">
						{#each client.redirect_uris ?? [] as uri (uri)}<li class="catalog-id">{uri}</li>{/each}
					</ul>
					<p class="catalog-help">
						Darkhorse returns the user to one of these exact URLs after authorization.
					</p>
				</dd>
			</div>
			<div>
				<dt>Refresh tokens</dt>
				<dd>
					{client.refresh_tokens ? 'Allowed' : 'Disabled'}
					<p class="catalog-help">
						{client.refresh_tokens
							? 'This client may renew access using rotating refresh tokens while the user’s session remains valid.'
							: 'This client must start another authorization flow when it needs new access tokens.'}
					</p>
				</dd>
			</div>
		</dl>
	</section>
	<details class="catalog-disclosure">
		<summary
			>Protected API allowances <span
				>{client.resource_ids?.length ?? 0} resources · {client.scope_ids?.length ?? 0} scopes</span
			></summary
		>
		<div>
			<p class="catalog-help">
				These allowlists limit what the client may request. They do not grant permissions to users.
				Identity-only sign-in can leave both empty and use the built-in <code>openid</code>,
				<code>profile</code>
				and <code>email</code> scopes.
			</p>
			<h4>Allowed resource IDs</h4>
			<ul class="catalog-values">
				{#each client.resource_ids ?? [] as id (id)}<li class="catalog-id">{id}</li>{:else}<li
						class="catalog-help"
					>
						No protected resources allowed.
					</li>{/each}
			</ul>
			<h4>Allowed scope IDs</h4>
			<ul class="catalog-values">
				{#each client.scope_ids ?? [] as id (id)}<li class="catalog-id">{id}</li>{:else}<li
						class="catalog-help"
					>
						No custom resource scopes allowed.
					</li>{/each}
			</ul>
		</div>
	</details>
</div>
