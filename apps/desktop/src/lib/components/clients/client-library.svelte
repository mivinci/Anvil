<script lang="ts">
	import { onMount } from 'svelte';
	import { Gamepad2, LoaderCircle, Plus, RefreshCw, Search, SearchX, TriangleAlert } from '@lucide/svelte';
	import { getClientLibraryState } from '../../client-library-state.svelte.js';
	const library = getClientLibraryState();
	import ClientCard from './client-card.svelte';
	import { getClientInstallState } from '../../client-install-state.svelte.js';
	const installs = getClientInstallState();
	import ClientInstallDialog from './client-install-dialog.svelte';
	let installOpen = $state(false);
	const filtered = $derived(library.clients.filter((client) => `${client.name} ${client.versionId}`.toLowerCase().includes(library.query.trim().toLowerCase())));
	onMount(() => { if (!library.loaded) void library.refresh(); });
</script>

<section class="flex min-h-0 flex-1 flex-col" aria-labelledby="clients-title">
	<header class="shrink-0 px-5 pt-6 pb-4 lg:px-7">
		<div class="flex items-center justify-between gap-4">
			<div class="flex items-baseline gap-2"><h1 id="clients-title" class="text-xl font-semibold tracking-tight">客户端</h1>{#if library.loaded}<span class="text-[11px] text-muted-foreground">{library.clients.length} 个实例</span>{/if}</div>
			<button type="button" data-client-install-trigger onclick={() => { if (installs.active || installs.starting || installs.uncertain) installs.detailsOpen = true; else installOpen = true; }} class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground disabled:opacity-40"><Plus size={14} aria-hidden="true" /> {installs.active || installs.starting ? '安装进度' : '安装版本'}</button>
		</div>
		<p class="mt-2 text-[11px] text-muted-foreground">管理你的 Minecraft Java 游戏实例</p>
		<div class="mt-5 flex items-center gap-2">
			<label class="flex h-8 w-full max-w-xs items-center gap-2 rounded-lg bg-chrome-hover px-2.5 text-muted-foreground focus-within:ring-1 focus-within:ring-ring"><Search size={14} class="shrink-0" aria-hidden="true" /><input type="search" bind:value={library.query} placeholder="搜索名称或版本" aria-label="搜索客户端" class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground" /></label>
			<button type="button" aria-label="刷新客户端列表" title="刷新" disabled={library.loading} onclick={() => void library.refresh()} class="grid size-8 shrink-0 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={14} class={library.loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button>
		</div>
	</header>
	<div class="min-h-0 flex-1 overflow-y-auto overscroll-contain px-5 pb-6 lg:px-7" aria-busy={library.loading}>
		{#if library.error}
			<div class="mb-4 rounded-lg border border-destructive/15 bg-destructive/5 px-3 py-2.5 text-xs" role="alert"><div class="flex items-center gap-2 text-destructive"><TriangleAlert size={14} class="shrink-0" aria-hidden="true" /><p class="flex-1">{library.loaded ? '刷新失败，已确认的实例仍保留。' : '暂时无法读取客户端列表。'}</p><button type="button" disabled={library.loading} onclick={() => void library.refresh()} class="shrink-0 rounded px-1 py-0.5 underline underline-offset-2 focus-visible:outline-2 focus-visible:outline-ring">重试</button></div><details class="mt-1.5 text-[11px] text-muted-foreground"><summary class="cursor-default">错误详情</summary><p class="mt-1 select-text wrap-break-word">{library.error}</p></details></div>
		{/if}
		{#if !library.loaded && library.loading}
			<div class="flex min-h-56 items-center justify-center gap-2 text-xs text-muted-foreground" role="status"><LoaderCircle size={16} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在读取客户端…</div>
		{:else if library.loaded && library.clients.length === 0}
			<div class="flex min-h-64 flex-col items-center justify-center text-center"><div class="mb-4 grid size-12 place-items-center rounded-2xl bg-chrome-hover text-muted-foreground"><Gamepad2 size={24} strokeWidth={1.5} aria-hidden="true" /></div><h2 class="text-[13px] font-medium">还没有客户端</h2><p class="mt-2 text-[11px] leading-5 text-muted-foreground">安装一个 Minecraft 版本，开始你的第一个世界。</p></div>
		{:else if library.loaded && filtered.length === 0}
			<div class="flex min-h-56 flex-col items-center justify-center gap-3 text-muted-foreground"><SearchX size={24} strokeWidth={1.5} aria-hidden="true" /><p class="text-xs">没有匹配的客户端</p><button type="button" onclick={() => library.query = ''} class="rounded px-2 py-1 text-xs text-primary hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">清除搜索</button></div>
		{:else if library.loaded}
			<ul class="grid grid-cols-[repeat(auto-fill,minmax(min(100%,210px),1fr))] items-start gap-3" aria-label="已安装客户端">{#each filtered as client (client.id)}<ClientCard {client} />{/each}</ul>
		{/if}
	</div>
</section>

{#if installOpen}<ClientInstallDialog clients={library.clients} onClose={() => installOpen = false} />{/if}
