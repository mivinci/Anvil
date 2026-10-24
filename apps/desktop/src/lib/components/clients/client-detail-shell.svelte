<script lang="ts">
	import { type Snippet } from 'svelte';
	import PageSidebar from '../ui/page-sidebar.svelte';
	import { pageSidebarItemClass } from '../ui/page-sidebar-item';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { Box, Boxes, Image, FileText, LoaderCircle, Settings2, Sparkles } from '@lucide/svelte';
	import { ClientDetailState, setClientDetailState } from '../../client-detail-state.svelte.js';
	import { getPageSidebarState } from '../../page-sidebar-state.svelte.js';
	let { children }: { children: Snippet } = $props();
	const detail = setClientDetailState(new ClientDetailState());
	const sidebar = getPageSidebarState();
	const name = $derived(page.params.name ?? '');
	const valid = $derived(/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,47}$/.test(name) && !name.endsWith('.'));
	const client = $derived(detail.client);
	const items = $derived([
		{ href: resolve('/clients/[name]', { name }), label: '概览', icon: Box },
		{ href: resolve('/clients/[name]/mods', { name }), label: '模组', icon: Boxes },
		{ href: resolve('/clients/[name]/resource-packs', { name }), label: '资源包', icon: Image },
		{ href: resolve('/clients/[name]/shader-packs', { name }), label: '光影', icon: Sparkles },
		{ href: resolve('/clients/[name]/logs', { name }), label: '游戏日志', icon: FileText },
		{ href: resolve('/clients/[name]/settings', { name }), label: '实例设置', icon: Settings2 },
	]);
	$effect(() => { if (valid) void detail.load(name); else detail.client = null; return () => detail.cancel(); });
</script>

<div class="flex min-h-0 flex-1 overflow-hidden">
	<PageSidebar open={sidebar.clients} id="client-detail-sidebar" label="实例侧栏">
		<div class="min-w-0 w-40 shrink-0 px-2 pb-4"><p class="truncate text-[13px] font-semibold" title={client?.name}>{client?.name ?? '实例详情'}</p><p class="mt-1 truncate text-[11px] text-muted-foreground">{client ? `Minecraft ${client.versionId}` : 'Minecraft Java'}</p></div>
		<nav aria-label="实例导航" class="w-40 shrink-0 space-y-1">
			{#each items as item (item.href)}
				<a href={item.href} aria-current={page.url.pathname === item.href ? 'page' : undefined} class={pageSidebarItemClass(page.url.pathname === item.href)}><item.icon size={15} strokeWidth={1.7} aria-hidden="true" />{item.label}</a>
			{/each}
		</nav>
	</PageSidebar>
	<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
		{#if !valid}
			<div class="px-5 py-4 text-[13px] text-muted-foreground" role="alert">实例名称无效。<a href={resolve('/clients')} class="ml-2 text-primary underline underline-offset-2">返回客户端</a></div>
		{:else if detail.loading && !client}
			<div class="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground" role="status"><LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />正在读取实例…</div>
		{:else if !client}
			<div class="px-5 py-4 text-[13px] text-muted-foreground" role="alert">{detail.error ?? '未找到这个实例。'}<button type="button" disabled={detail.loading} onclick={() => void detail.load(name)} class="ml-2 text-primary underline underline-offset-2 disabled:opacity-40">重新读取</button><a href={resolve('/clients')} class="ml-2 text-primary underline underline-offset-2">返回客户端</a></div>
		{:else}
			{#if detail.error}<p role="alert" class="px-5 pt-3 text-[11px] text-destructive">刷新失败，保留上次读取的信息。<button type="button" disabled={detail.loading} onclick={() => void detail.load(name)} class="ml-2 underline underline-offset-2">重试</button></p>{/if}
			{@render children()}
		{/if}
	</div>
</div>
