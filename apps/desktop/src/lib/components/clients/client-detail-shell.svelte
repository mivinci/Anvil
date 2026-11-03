<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { type Snippet } from 'svelte';
	import { validClientName } from '../../client-name.ts';
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
	const valid = $derived(validClientName(name));
	const client = $derived(detail.client);
	const items = $derived([
		{ href: resolve('/clients/[name]', { name }), label: localeState.messages.instance.detail.overview, icon: Box },
		{ href: resolve('/clients/[name]/mods', { name }), label: localeState.messages.instance.detail.mods, icon: Boxes },
		{ href: resolve('/clients/[name]/shader-packs', { name }), label: localeState.messages.instance.detail.shaders, icon: Sparkles },
		{ href: resolve('/clients/[name]/resource-packs', { name }), label: localeState.messages.instance.detail.resourcePacks, icon: Image },
		{ href: resolve('/clients/[name]/logs', { name }), label: localeState.messages.instance.detail.gameLog, icon: FileText },
		{ href: resolve('/clients/[name]/settings', { name }), label: localeState.messages.instance.detail.settings, icon: Settings2 },
	]);
	$effect(() => { if (valid) void detail.load(name); else detail.client = null; return () => detail.cancel(); });
</script>

<div class="flex min-h-0 flex-1 overflow-hidden">
	<PageSidebar open={sidebar.clients} id="client-detail-sidebar" label={localeState.messages.instance.detail.instanceSidebar}>
		<div class="min-w-0 w-40 shrink-0 px-2 pb-4"><p class="truncate text-[13px] font-semibold" title={client?.name}>{client?.name ?? localeState.messages.instance.detail.details}</p><p class="mt-1 truncate text-[12px] text-muted-foreground">{client ? `Minecraft ${client.versionId}` : 'Minecraft Java'}</p></div>
		<nav aria-label={localeState.messages.instance.detail.navigation} class="w-40 shrink-0 space-y-1">
			{#each items as item (item.href)}
				<a href={item.href} aria-current={page.url.pathname === item.href ? 'page' : undefined} class={pageSidebarItemClass(page.url.pathname === item.href)}><item.icon size={15} strokeWidth={1.7} aria-hidden="true" />{item.label}</a>
			{/each}
		</nav>
	</PageSidebar>
	<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
		{#if !valid}
			<div class="px-5 py-4 text-[13px] text-muted-foreground" role="alert">{localeState.messages.instance.detail.invalidName}<a href={resolve('/clients')} class="ml-2 text-primary underline underline-offset-2">{localeState.messages.instance.detail.backClients}</a></div>
		{:else if detail.loading && !client}
			<div class="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground" role="status"><LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.instance.detail.loadingInstance}</div>
		{:else if !client}
			<div class="px-5 py-4 text-[13px] text-muted-foreground" role="alert">{detail.error ?? localeState.messages.instance.detail.notFound}<button type="button" disabled={detail.loading} onclick={() => void detail.load(name)} class="ml-2 text-primary underline underline-offset-2 disabled:opacity-40">{localeState.messages.instance.detail.reload}</button><a href={resolve('/clients')} class="ml-2 text-primary underline underline-offset-2">{localeState.messages.instance.detail.backClients}</a></div>
		{:else}
			{#if detail.error}<p role="alert" class="px-5 pt-3 text-[12px] text-destructive">{localeState.messages.instance.detail.refreshKeep}<button type="button" disabled={detail.loading} onclick={() => void detail.load(name)} class="ml-2 underline underline-offset-2">{localeState.messages.instance.detail.retry}</button></p>{/if}
			{@render children()}
		{/if}
	</div>
</div>
