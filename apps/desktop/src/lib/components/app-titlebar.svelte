<script lang="ts">
	import { afterNavigate, goto } from '$app/navigation';
	import { ArrowLeft, ArrowRight, PanelLeft } from '@lucide/svelte';
	import { onMount } from 'svelte';
	import { isTauri } from '@tauri-apps/api/core';
	let windows = $state(false);
	onMount(() => {
		if (!isTauri() || !navigator.userAgent.includes('Windows')) return;
		windows = true;
	});
	import { page } from '$app/state';
	import { getPageSidebarState } from '../page-sidebar-state.svelte.js';
	const sidebar = getPageSidebarState();
	const sidebarArea = $derived(page.params.name !== undefined && page.route.id?.startsWith('/clients/') ? 'clients' : page.route.id === '/mods' ? 'mods' : page.route.id === '/shaders' ? 'shaders' : null);
	const sidebarLabel = $derived(sidebarArea === 'mods' ? '模组筛选侧栏' : sidebarArea === 'shaders' ? '光影筛选侧栏' : '实例侧栏');
	const sidebarId = $derived(sidebarArea === 'mods' ? 'mod-discovery-sidebar' : sidebarArea === 'shaders' ? 'shader-discovery-sidebar' : 'client-detail-sidebar');
	import DownloadTasks from './download-tasks.svelte';

	let historyPosition = $state(0);
	let historyEnd = $state(0);
	let historyPending = $state(false);
	let currentUrl: string | undefined;
	let replacingProject = false;
	const directModDetail = $derived(page.route.id === '/mods' && page.url.searchParams.has('project'));

	afterNavigate((navigation) => {
		historyPending = false;
		const nextUrl = navigation.to?.url.href;
		if (replacingProject) { replacingProject = false; currentUrl = nextUrl; return; }
		if (navigation.type === 'enter') {
			currentUrl = nextUrl;
			return;
		}

		if (navigation.type === 'popstate') {
			historyPosition = Math.max(0, historyPosition + navigation.delta);
		} else if (nextUrl !== currentUrl) {
			historyPosition += 1;
			historyEnd = historyPosition;
		}
		currentUrl = nextUrl;
	});

	function traverseHistory(direction: -1 | 1) {
		if (historyPending || (direction === -1 ? historyPosition === 0 && !directModDetail : historyPosition >= historyEnd)) {
			return;
		}
		historyPending = true;
		if (direction === -1 && historyPosition === 0 && directModDetail) {
			const next = new URL(page.url.href);
			next.searchParams.delete('project');
			replacingProject = true;
			void goto(`${next.pathname}${next.search}`, { replace: true, reset: false }).catch(() => { replacingProject = false; historyPending = false; });
			return;
		}
		window.history.go(direction);
	}
</script>

<header
	class={`relative z-10 flex h-8 shrink-0 items-center gap-1 bg-transparent cursor-grab ${windows ? 'pl-2.5' : 'pl-21 pr-2'}`}
	data-tauri-drag-region
	aria-label="窗口拖动区域"
>
	<button
		type="button"
		class="grid size-6 cursor-default place-items-center rounded-md text-chrome-muted/80 transition-colors enabled:hover:bg-chrome-selected enabled:hover:text-chrome-muted focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:opacity-30"
		aria-label="后退"
		title="后退"
		disabled={historyPending || (historyPosition === 0 && !directModDetail)}
		onclick={() => traverseHistory(-1)}
	><ArrowLeft size={16} aria-hidden="true" />
	</button>
	<button
		type="button"
		class="grid size-6 cursor-default place-items-center rounded-md text-chrome-muted/80 transition-colors enabled:hover:bg-chrome-selected enabled:hover:text-chrome-muted focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:opacity-30"
		aria-label="前进"
		title="前进"
		disabled={historyPending || historyPosition >= historyEnd}
		onclick={() => traverseHistory(1)}
	><ArrowRight size={16} aria-hidden="true" />
	</button>
	{#if sidebarArea}
		<button type="button" aria-label={`${sidebar[sidebarArea] ? '收起' : '展开'}${sidebarLabel}`} title={`${sidebar[sidebarArea] ? '收起' : '展开'}${sidebarLabel}`} aria-expanded={sidebar[sidebarArea]} aria-controls={sidebarId} onclick={() => sidebar.toggle(sidebarArea!)} class="ml-1 grid size-6 cursor-default place-items-center rounded-md text-chrome-muted/80 transition-colors hover:bg-chrome-selected hover:text-chrome-muted focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"><PanelLeft size={16} aria-hidden="true" /></button>
	{/if}
	<div class="ml-auto flex items-center">
		<DownloadTasks />
	</div>
	{#if windows}
		<!-- Native DWM caption buttons occupy this area above the WebView. -->
		<div class="h-full w-34.5 shrink-0" aria-hidden="true"></div>
	{/if}
</header>

