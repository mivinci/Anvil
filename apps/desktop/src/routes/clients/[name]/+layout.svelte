<script lang="ts">
	import type { Snippet } from 'svelte';
	import ClientDetailShell from '#lib/components/clients/client-detail-shell.svelte';
	let { children }: { children: Snippet } = $props();
</script>
<ClientDetailShell>{@render children()}</ClientDetailShell>
