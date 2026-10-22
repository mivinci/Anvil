<script lang="ts">
	import type { Snippet } from 'svelte';
	import '../app.css';
	import AppShell from '#lib/components/app-shell.svelte';

	let { children }: { children: Snippet } = $props();
</script>

<svelte:head>
	<title>Anvil 本地</title>
</svelte:head>

<AppShell>
	{@render children()}
</AppShell>
