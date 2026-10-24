<script lang="ts">
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { Gamepad2, ChevronRight, Plus, RefreshCw, Blocks, Clock3, Layers } from '@lucide/svelte';
	import { getClientLibraryState } from '../client-library-state.svelte.js';
	import { getClientLaunchState, launchLabel } from '../client-launch-state.svelte.js';
	import ClientLaunchControls from './clients/client-launch-controls.svelte';
	import ClientArtwork from './clients/client-artwork.svelte';
	const library = getClientLibraryState();
	const launches = getClientLaunchState();
	const ordered = $derived([...library.clients].sort((a, b) => {
		const active = (name: string) => launches.forClient(name)?.phase === 'running';
		const time = (name: string) => launches.recentPlays.find(value => value.name === name.toLowerCase())?.playedAt ?? 0;
		return Number(active(b.name)) - Number(active(a.name)) || time(b.name) - time(a.name);
	}));
	const featured = $derived(ordered[0]);
	const recent = $derived(featured ? launches.recentPlays.find(value => value.name === featured.name.toLowerCase()) : null);
	const shortcuts = $derived(ordered.slice(1, 4));
	const link = 'inline-flex items-center gap-1 rounded-md text-[11px] text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring';
	onMount(() => { void library.refresh(); });
</script>

<div class={`grid min-w-0 gap-3 ${shortcuts.length ? 'min-[860px]:grid-cols-2' : ''}`}>
	<section aria-labelledby="home-play-title" class="flex min-w-0 flex-col overflow-hidden rounded-lg border border-border bg-card p-3.5">
		<div class="flex items-center justify-between gap-3">
			<h2 id="home-play-title" class="flex items-center gap-2 text-[13px] font-semibold"><Gamepad2 size={15} class="text-primary" aria-hidden="true" />最近游玩</h2>
			{#if featured}<a href={resolve('/clients/[name]', { name: featured.name.toLowerCase() })} class={link}>管理实例<ChevronRight size={12} aria-hidden="true" /></a>{/if}
		</div>
		{#if library.error}
			<div role="alert" class="mt-3 flex items-center justify-between gap-2 text-[11px] text-muted-foreground"><span>{library.loaded ? '刷新失败，保留已有实例' : '暂时无法读取实例'}</span><button type="button" disabled={library.loading} class={link} onclick={() => void library.refresh()}><RefreshCw size={12} aria-hidden="true" />重试</button></div>
		{/if}
		{#if featured}
			<div class="mt-3 flex items-center gap-3">
				<div class="min-w-0 flex-1">
					<a href={resolve('/clients/[name]', { name: featured.name.toLowerCase() })} class="block truncate rounded text-base font-semibold hover:text-primary focus-visible:outline-2 focus-visible:outline-ring" title={featured.name}>{featured.name}</a>
					<p class="mt-1 truncate text-[11px] text-muted-foreground">Minecraft {featured.versionId} · {featured.loader ? `Fabric ${featured.loader.version}` : '原版'}</p>
					{#if recent}<p class="mt-2 flex items-center gap-1 text-[10px] text-muted-foreground"><Clock3 size={11} aria-hidden="true" />{new Intl.DateTimeFormat('zh-CN', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(recent.playedAt)}</p>{:else}<p class="mt-2 text-[10px] text-muted-foreground">尚无游玩记录</p>{/if}
				</div>
				<div class="h-18 w-32 shrink-0 text-primary max-[840px]:hidden"><ClientArtwork id={featured.id} /></div>
			</div>
			<div class="mt-auto pt-3"><ClientLaunchControls client={featured} /></div>
		{:else if !library.loaded && !library.error}
			<p role="status" class="py-7 text-center text-xs text-muted-foreground">正在读取实例…</p>
		{:else if library.loaded}
			<div class="py-5"><p class="text-[13px] font-medium">创建你的第一个世界</p><p class="mt-1 text-[11px] text-muted-foreground">先安装一个 Minecraft 版本，就可以开始游玩。</p><a href={resolve('/clients')} class="mt-4 inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring"><Plus size={13} aria-hidden="true" />前往创建实例</a></div>
		{/if}
	</section>
	{#if shortcuts.length}
		<section aria-labelledby="home-instances-title" class="min-w-0 rounded-lg border border-border bg-card p-3.5">
			<div class="mb-2 flex items-center justify-between gap-3"><h2 id="home-instances-title" class="flex items-center gap-2 text-[13px] font-semibold"><Layers size={15} class="text-primary" aria-hidden="true" />实例快捷入口</h2><a href={resolve('/clients')} class={link}>全部实例<ChevronRight size={12} aria-hidden="true" /></a></div>
			<ul class="space-y-0.5">{#each shortcuts as client (client.id)}
				<li class="flex min-w-0 flex-wrap items-center gap-x-2 rounded-md px-1.5 py-2 hover:bg-chrome-hover">
					<a href={resolve('/clients/[name]', { name: client.name.toLowerCase() })} class="flex min-w-0 flex-1 items-center gap-2.5 rounded focus-visible:outline-2 focus-visible:outline-ring"><span class="grid size-8 shrink-0 place-items-center rounded-lg bg-primary/10 text-primary"><Gamepad2 size={15} aria-hidden="true" /></span><span class="min-w-0"><span class="block truncate text-xs font-medium" title={client.name}>{client.name}</span><span class="mt-0.5 block truncate text-[10px] text-muted-foreground">{client.versionId} · {client.loader ? 'Fabric' : '原版'}{launches.forClient(client.name)?.phase === 'running' ? ` · ${launchLabel(launches.forClient(client.name)!)}` : ''}</span></span></a>
					<ClientLaunchControls {client} compact />
				</li>
			{/each}</ul>
		</section>
	{/if}
	<a href={resolve('/mods')} class="group col-span-full flex items-center justify-between rounded-lg border border-border bg-card px-3.5 py-3 text-xs focus-visible:outline-2 focus-visible:outline-ring"><span class="flex items-center gap-2"><Blocks size={15} class="text-primary" aria-hidden="true" />发现模组</span><ChevronRight size={13} class="text-muted-foreground group-hover:text-foreground" aria-hidden="true" /></a>
</div>
