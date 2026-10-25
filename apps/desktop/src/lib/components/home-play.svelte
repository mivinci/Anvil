<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { Gamepad2, ChevronRight, Plus, RefreshCw, Clock3 } from '@lucide/svelte';
	import { getClientLibraryState } from '../client-library-state.svelte.js';
	import { getClientLaunchState } from '../client-launch-state.svelte.js';
	import ClientLaunchControls from './clients/client-launch-controls.svelte';
	import ClientArtwork from './clients/client-artwork.svelte';
	import ClientCard from './clients/client-card.svelte';
	let { part = 'all' }: { part?: 'continue' | 'instances' | 'all' } = $props();
	const library = getClientLibraryState();
	const launches = getClientLaunchState();
	const showContinue = $derived(part !== 'instances');
	const showInstances = $derived(part !== 'continue');
	const ordered = $derived([...library.clients].sort((a, b) => {
		const active = (name: string) => launches.forClient(name)?.phase === 'running';
		const time = (name: string) => launches.recentPlays.find(value => value.name === name.toLowerCase())?.playedAt ?? 0;
		return Number(active(b.name)) - Number(active(a.name)) || time(b.name) - time(a.name);
	}));
	const featured = $derived(ordered[0]);
	const recent = $derived(featured ? launches.recentPlays.find(value => value.name === featured.name.toLowerCase()) : null);
	const link = 'inline-flex items-center gap-1 rounded-md text-[11px] text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring';
	onMount(() => { void library.refresh(); });
</script>

<div class={`min-w-0 space-y-3 ${part === 'continue' ? 'h-full' : ''}`}>
	{#if showContinue && library.error}
		<div role="alert" class="flex items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2 text-[11px] text-muted-foreground">
			<span>{library.loaded ? '刷新失败，保留已有实例' : '暂时无法读取实例'}</span>
			<button type="button" disabled={library.loading} class={link} onclick={() => void library.refresh()}><RefreshCw size={12} aria-hidden="true" />重试</button>
		</div>
	{/if}

	{#if showContinue && featured}
		<section class="h-full rounded-lg border border-border bg-card" aria-labelledby="home-play-title">
			<div class="flex h-full flex-col px-4 pt-3.5 pb-3">
				<div class="flex items-center justify-between gap-3">
					<h2 id="home-play-title" class="flex items-center gap-2 text-[13px] font-semibold"><Gamepad2 size={15} class="text-primary" aria-hidden="true" />最近游玩</h2>
					<a href={resolve('/clients/[name]', { name: featured.name.toLowerCase() })} class={link}>管理实例<ChevronRight size={12} aria-hidden="true" /></a>
				</div>
				<div class="flex flex-1 items-end justify-between gap-5 pt-4">
					<div class="min-w-0 flex-1">
						<a href={resolve('/clients/[name]', { name: featured.name.toLowerCase() })} class="block truncate rounded text-2xl font-semibold tracking-tight hover:text-primary focus-visible:outline-2 focus-visible:outline-ring" title={featured.name}>{featured.name}</a>
						<p class="mt-1 truncate text-xs text-muted-foreground">Minecraft {featured.versionId} · {featured.loader ? `Fabric ${featured.loader.version}` : '原版'}</p>
						{#if recent}
							<p class="mt-2 flex items-center gap-1 text-[10px] text-muted-foreground"><Clock3 size={11} aria-hidden="true" />{new Intl.DateTimeFormat('zh-CN', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(recent.playedAt)}</p>
						{:else}
							<p class="mt-2 text-[10px] text-muted-foreground">尚无游玩记录</p>
						{/if}
						<div class="mt-5"><ClientLaunchControls client={featured} large /></div>
					</div>
					<div class="hidden h-28 w-64 shrink-0 self-center text-primary min-[560px]:block" aria-hidden="true"><ClientArtwork id={featured.id} /></div>
				</div>
			</div>
		</section>
	{/if}

	{#if showInstances && featured}
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

	{#if showContinue && !featured && !library.loaded && !library.error}
		<section class="rounded-lg border border-border bg-card px-4 py-12 text-center" role="status">
			<p class="text-xs text-muted-foreground">正在读取实例…</p>
		</section>
	{:else if showContinue && !featured && library.loaded}
		<section class="rounded-lg border border-border bg-card px-4 py-10">
			<p class="text-[13px] font-medium">创建你的第一个世界</p>
			<p class="mt-1 text-[11px] text-muted-foreground">先安装一个 Minecraft 版本，就可以开始游玩。</p>
			<a href={resolve('/clients')} class="mt-4 inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring"><Plus size={13} aria-hidden="true" />前往创建实例</a>
		</section>
	{/if}
</div>
