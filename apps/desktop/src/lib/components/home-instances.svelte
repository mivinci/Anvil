<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { ChevronRight } from '@lucide/svelte';
	import { getClientLibraryState } from '../client-library-state.svelte.js';
	import { getClientLaunchState } from '../client-launch-state.svelte.js';
	import ClientCard from './clients/client-card.svelte';
	const library = getClientLibraryState();
	const launches = getClientLaunchState();
	const ordered = $derived([...library.clients].sort((a, b) => {
		const active = (name: string) => launches.forClient(name)?.phase === 'running';
		const time = (name: string) => launches.recentPlays.find(value => value.name === name.toLowerCase())?.playedAt ?? 0;
		return Number(active(b.name)) - Number(active(a.name)) || time(b.name) - time(a.name);
	}));
	const link = 'inline-flex items-center gap-1 rounded-md text-[11px] text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring';
	onMount(() => { void library.refresh(); });
</script>

{#if ordered.length > 0}
	<section aria-labelledby="home-instances-title" class="min-w-0">
		<div class="mb-2 flex items-center justify-between gap-3 px-1">
			<h2 id="home-instances-title" class="text-[13px] font-semibold">全部实例 <span class="font-normal text-muted-foreground">{library.clients.length}</span></h2>
			<a href={resolve('/clients')} class={link}>管理全部<ChevronRight size={12} aria-hidden="true" /></a>
		</div>
		<ul class="grid grid-cols-[repeat(auto-fill,minmax(min(100%,210px),1fr))] items-start gap-3" aria-label="已安装实例">
			{#each ordered as client (client.id)}<ClientCard {client} />{/each}
		</ul>
	</section>
{/if}
