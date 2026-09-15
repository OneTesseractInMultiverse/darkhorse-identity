<script lang="ts">
	import { resolve } from '$app/paths';
	import ConnectionCheck from '$lib/components/ConnectionCheck.svelte';
	import { checkHealth } from '$lib/health';

	async function check() {
		return checkHealth(fetch);
	}
</script>

<svelte:head>
	<title>Darkhorse — Identity console</title>
	<meta name="description" content="Darkhorse identity management console" />
</svelte:head>

<div class="portal">
	<header class="topbar">
		<a href={resolve('/')} class="wordmark" aria-label="Darkhorse home"
			><span class="mark" aria-hidden="true">D/</span> DARKHORSE</a
		>
		<span class="edition">IDENTITY CONSOLE <span class="version">/ 0.1</span></span>
	</header>

	<main id="main">
		<section class="glass" aria-labelledby="welcome">
			<div class="panel-top">
				<span><span class="dot"></span> WORKSPACE</span><span>01 / FOUNDATION</span>
			</div>
			<div class="panel-body">
				<span class="eyebrow">YOUR ORGANIZATION. ONE IDENTITY.</span>
				<h1 id="welcome">Access begins<br />with <span>trust.</span></h1>
				<p class="intro">A shared home for your people, applications, and access.</p>
				<div class="notice">
					<span aria-hidden="true">&gt;_</span>
					<p>
						The workspace is taking shape.<br /><span>Identity services are under development.</span
						>
					</p>
				</div>
				<div class="connection"><ConnectionCheck {check} /></div>
			</div>
		</section>
		<p class="caption">A CLEAR VIEW. A CONTROLLED PATH.</p>
	</main>

	<footer><span>DARKHORSE / IDENTITY SYSTEMS</span><span>FOUNDATION PREVIEW</span></footer>
</div>
