<script lang="ts">
	import { resolve } from '$app/paths';
	import ClientArtwork from './client-artwork.svelte';
	import ClientLaunchControls from './client-launch-controls.svelte';
	import type { InstalledClient } from '../../runtime';
	import { getClientLaunchState, launchActive, launchLabel, launchNeedsLogin } from '../../client-launch-state.svelte.js';
	let { client }: { client: InstalledClient } = $props();
	const launches = getClientLaunchState();
	const operation = $derived(launches.forClient(client.name));
	const active = $derived(launchActive(operation));
	const label = $derived(operation ? launchLabel(operation) : '启动');
	const downloading = $derived(active && operation?.javaStage === 'downloading');
	const progress = $derived(downloading && operation?.javaTotalBytes ? Math.min(100, operation.javaDownloadedBytes / operation.javaTotalBytes * 100) : null);
	const statusBadge = $derived(
		operation?.phase === 'running' ? (operation.offlineTest ? '离线测试' : '运行中')
			: launchNeedsLogin(operation) ? '需要重新登录'
			: operation?.phase === 'failed' ? '启动失败'
				: operation?.phase === 'cancelled' ? '已取消'
					: operation?.phase === 'stopping' ? '停止中'
						: null,
	);
	const badgeTone = $derived(operation?.phase === 'failed' ? 'text-destructive' : operation?.phase === 'cancelled' ? 'text-muted-foreground' : 'text-foreground');
	const motif = $derived(Array.from(client.id).reduce((hash, character) => (hash * 31 + character.charCodeAt(0)) >>> 0, 0) % 3);
	const palette = $derived(['bg-primary/10 text-primary', 'bg-teal-600/8 text-teal-700/70 dark:text-teal-300/60', 'bg-stone-500/10 text-stone-600/80 dark:text-stone-300/60'][motif]);
</script>
<li class="min-w-0">
	<div class="group relative isolate overflow-hidden rounded-xl bg-card transition-[transform,border-color,box-shadow] duration-200 hover:border-primary/30 hover:shadow-sm motion-safe:hover:-translate-y-0.5">
		<a href={resolve('/clients/[name]', { name: client.name.toLowerCase() })} aria-label={`管理实例 ${client.name}`} class="absolute inset-0 z-10 rounded-xl focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"></a>
		<div class={`pointer-events-none relative flex h-23 items-center justify-center overflow-hidden ${palette}`}>
			<span class="absolute top-2.5 left-3 text-[10px] font-medium tracking-wide opacity-70">{client.loader ? 'FABRIC' : 'VANILLA'}</span>
			{#if statusBadge}<span class={`absolute top-2.5 right-3 rounded-full bg-card/80 px-2 py-0.5 text-[10px] ${badgeTone}`} title={label}>{statusBadge}</span>{/if}
			<div class="h-22 w-48 transition-transform duration-300 motion-safe:group-hover:-translate-y-1 motion-reduce:transition-none"><ClientArtwork id={client.id} /></div>
		</div>
		<div class="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-x-2 px-3 py-3">
			<div class="min-w-0 flex-1"><h2 class="truncate text-[13px] font-medium" title={client.name}>{client.name}</h2><p class="mt-1 truncate text-[11px] text-muted-foreground" title={client.loader ? `Minecraft ${client.versionId} · Fabric ${client.loader.version}` : `Minecraft ${client.versionId}`}>{client.versionId}{client.loader ? ` · Fabric ${client.loader.version}` : ' · 原版'}</p></div>
			<ClientLaunchControls {client} compact />
		</div>
		{#if downloading}<progress aria-label="Java 下载进度" max="100" value={progress ?? undefined} class="absolute inset-x-0 bottom-0 h-0.5 w-full accent-primary [&::-webkit-progress-bar]:bg-chrome-hover [&::-webkit-progress-value]:bg-primary"></progress>{/if}
	</div>
</li>
