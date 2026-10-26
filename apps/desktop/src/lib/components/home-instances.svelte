<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { ChevronRight, LoaderCircle, Plus, RefreshCw } from '@lucide/svelte';
	import { getClientLibraryState } from '../client-library-state.svelte.js';
	import { getClientLaunchState } from '../client-launch-state.svelte.js';
	import { getClientInstallState } from '../client-install-state.svelte.js';
	import ClientCard from './clients/client-card.svelte';
	import ClientArtwork from './clients/client-artwork.svelte';
	import ClientInstallDialog from './clients/client-install-dialog.svelte';
	const library = getClientLibraryState();
	const launches = getClientLaunchState();
	const installs = getClientInstallState();
	let installOpen = $state(false);
	const installing = $derived(installs.active || installs.starting);
	const ordered = $derived([...library.clients].sort((a, b) => {
		const active = (name: string) => launches.forClient(name)?.phase === 'running';
		const time = (name: string) => launches.recentPlays.find(value => value.name === name.toLowerCase())?.playedAt ?? 0;
		return Number(active(b.name)) - Number(active(a.name)) || time(b.name) - time(a.name);
	}));
	const link = 'inline-flex items-center gap-1 rounded-md text-[11px] text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring';
	function openInstall() {
		if (installing || installs.uncertain) installs.detailsOpen = true;
		else installOpen = true;
	}
	onMount(() => { void library.refresh(); });
</script>

<section aria-labelledby="home-instances-title" class="min-w-0" aria-busy={library.loading}>
		<div class="mb-2 flex items-center justify-between gap-3 px-1">
			<h2 id="home-instances-title" class="text-[13px] font-semibold">全部实例{#if library.loaded} <span class="font-normal text-muted-foreground">{library.clients.length}</span>{/if}</h2>
			<a href={resolve('/clients')} class={link}>管理全部<ChevronRight size={12} aria-hidden="true" /></a>
		</div>
		{#if library.error}
			<div class={`flex flex-wrap items-center justify-center gap-2 rounded-lg bg-card px-4 py-3 text-[11px] text-muted-foreground ${ordered.length ? 'mb-3' : 'min-h-40'}`} role="alert">
				<p>{library.loaded && ordered.length ? '刷新失败，保留已有实例。' : '暂时无法读取实例，请重试。'}</p>
				<button type="button" class={link} disabled={library.loading} onclick={() => void library.refresh()}><RefreshCw size={12} class={library.loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" />重新读取</button>
			</div>
		{/if}
		{#if ordered.length > 0}
		<ul class="grid grid-cols-[repeat(auto-fill,minmax(min(100%,210px),1fr))] items-start gap-3" aria-label="已安装实例">
			{#each ordered as client (client.id)}<ClientCard {client} />{/each}
		</ul>
		{:else if !library.error && !library.loaded}
			<div class="flex min-h-40 items-center justify-center gap-2 rounded-lg bg-card text-[11px] text-muted-foreground" role="status"><LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在读取实例…</div>
		{:else if !library.error}
			<div class="flex min-h-40 flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-primary/20 bg-card/50 px-5 py-5 text-center min-[600px]:flex-row min-[600px]:gap-5 min-[600px]:text-left">
				<div class="h-20 w-28 shrink-0 text-primary/45" aria-hidden="true"><ClientArtwork id="anvil-first-instance" /></div>
				<div class="min-w-0">
					<h3 class="text-[13px] font-medium">{installing ? '正在准备第一个实例' : '还没有游戏实例'}</h3>
					<p class="mt-1 text-[11px] leading-5 text-muted-foreground">{installing ? '安装完成后，它就会出现在这里。' : '安装一个 Minecraft 版本，开始你的第一个世界。'}</p>
					<button type="button" data-client-install-trigger onclick={openInstall} class="mt-3 inline-flex h-8 items-center justify-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 focus-visible:outline-2 focus-visible:outline-ring">{#if installing}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />安装进度{:else}<Plus size={13} aria-hidden="true" />安装版本{/if}</button>
				</div>
			</div>
		{/if}
</section>

{#if installOpen}<ClientInstallDialog clients={library.clients} onClose={() => installOpen = false} />{/if}
