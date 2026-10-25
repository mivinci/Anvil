<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { ChevronRight, Clock3, Gamepad2, Plus, RefreshCw } from '@lucide/svelte';
	import ClientArtwork from './clients/client-artwork.svelte';
	import ClientLaunchControls from './clients/client-launch-controls.svelte';
	import { getClientLibraryState } from '../client-library-state.svelte.js';
	import { getClientLaunchState } from '../client-launch-state.svelte.js';
	const library = getClientLibraryState();
	const launches = getClientLaunchState();
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

<section class="flex h-full min-w-0 flex-col rounded-lg bg-card p-3.5" aria-labelledby="home-play-title">
	{#if featured}
		<div class="flex items-center justify-between gap-3">
			<h2 id="home-play-title" class="flex items-center gap-2 text-[13px] font-semibold"><Gamepad2 size={15} class="text-primary" aria-hidden="true" />最近游玩</h2>
			<a href={resolve('/clients/[name]', { name: featured.name.toLowerCase() })} class={link}>管理实例<ChevronRight size={12} aria-hidden="true" /></a>
		</div>
		<div class="flex flex-1 items-center justify-between gap-5 pt-4">
			<div class="min-w-0 flex-1">
				<a href={resolve('/clients/[name]', { name: featured.name.toLowerCase() })} class="block truncate rounded text-2xl font-semibold tracking-tight hover:text-primary focus-visible:outline-2 focus-visible:outline-ring" title={featured.name}>{featured.name}</a>
				<p class="mt-1 truncate text-xs text-muted-foreground">Minecraft {featured.versionId} · {featured.loader ? `Fabric ${featured.loader.version}` : '原版'}</p>
				{#if recent}
					<p class="mt-2 flex items-center gap-1 text-[10px] text-muted-foreground"><Clock3 size={11} aria-hidden="true" />{new Intl.DateTimeFormat('zh-CN', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(recent.playedAt)}</p>
				{:else}
					<p class="mt-2 text-[10px] text-muted-foreground">尚无游玩记录</p>
				{/if}
				<div class="mt-4"><ClientLaunchControls client={featured} large /></div>
			</div>
			<div class="hidden h-24 w-44 shrink-0 self-center text-primary min-[1200px]:block" aria-hidden="true"><ClientArtwork id={featured.id} /></div>
		</div>
	{:else if library.error}
		<div class="flex flex-1 flex-col items-start justify-center gap-2" role="alert">
			<p class="text-[13px] font-medium">{library.loaded ? '刷新失败，保留已有实例' : '暂时无法读取实例'}</p>
			<button type="button" disabled={library.loading} class={link} onclick={() => void library.refresh()}><RefreshCw size={12} aria-hidden="true" />重试</button>
		</div>
	{:else if !library.loaded}
		<div class="flex flex-1 items-center justify-center text-xs text-muted-foreground" role="status">正在读取实例…</div>
	{:else}
		<div class="flex flex-1 flex-col justify-center">
			<p class="text-[13px] font-medium">创建你的第一个世界</p>
			<p class="mt-1 text-[11px] text-muted-foreground">先安装一个 Minecraft 版本，就可以开始游玩。</p>
			<a href={resolve('/clients')} class="mt-3 inline-flex h-8 w-fit items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring"><Plus size={13} aria-hidden="true" />前往创建实例</a>
		</div>
	{/if}
</section>
