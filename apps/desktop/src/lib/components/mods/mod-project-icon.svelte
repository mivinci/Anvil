<script lang="ts">
	import { Blocks } from '@lucide/svelte';
	import type { ModProjectSummary, ModSource } from '../../runtime';
	import { modProjectIcon } from '../../mod-project-icon';
	let { project, source, large = false }: { project: ModProjectSummary; source: ModSource; large?: boolean } = $props();
	const url = $derived(modProjectIcon(project, source));
	let failedUrl = $state<string | null>(null);
	let loadedUrl = $state<string | null>(null);
</script>

<span class={`relative grid shrink-0 place-items-center overflow-hidden rounded-lg bg-chrome-hover text-muted-foreground ${large ? 'size-12' : 'size-9'}`} aria-hidden="true">
	<Blocks size={large ? 22 : 16} strokeWidth={1.5} class={url && loadedUrl === url && failedUrl !== url ? 'opacity-0' : ''} />
	{#if url && failedUrl !== url}
		<img src={url} alt="" width={large ? 48 : 36} height={large ? 48 : 36} loading="lazy" decoding="async" referrerpolicy="no-referrer" class="absolute inset-0 size-full object-contain" onload={event => loadedUrl = (event.currentTarget as HTMLImageElement).src} onerror={event => failedUrl = (event.currentTarget as HTMLImageElement).src} />
	{/if}
</span>
