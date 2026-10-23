<script lang="ts">
	import { Pause, Play, RotateCcw, ArrowUpRight, X } from '@lucide/svelte';
	import { formatBytes } from '../../download-progress';
	import { resolve } from '$app/paths';
	import { getClientLibraryState } from '../../client-library-state.svelte.js';
	let { onNavigate }: { onNavigate?: () => void } = $props();
	const library = getClientLibraryState();
	import { getClientInstallState } from '../../client-install-state.svelte.js';
	const installs = getClientInstallState();
	const operation = $derived(installs.operation);
	const files = $derived(operation?.progress.totalFiles ?? null);
	const totalBytes = $derived(operation?.progress.totalBytes ?? null);
	const downloadedBytes = $derived(operation?.progress.downloadedBytes ?? 0);
	const percent = $derived(totalBytes && operation ? Math.min(100, Math.max(0, downloadedBytes / totalBytes * 100)) : files && operation ? Math.min(100, Math.max(0, operation.progress.completedFiles / files * 100)) : null);
	const showSpeed = $derived(operation?.state === 'running' && operation.progress.stage === 'downloading' && !operation.cancelRequested && !installs.error && !installs.syncing && !installs.changingPause);
</script>

<div class="space-y-3">
	<div><p class="truncate text-[13px] font-medium">{operation?.name ?? installs.intent?.name ?? 'Minecraft'}</p><p class="mt-1 text-[11px] text-muted-foreground">Minecraft {operation?.versionId ?? installs.intent?.versionId ?? '—'}{operation?.loader ? ` · Fabric ${operation.loader.version}` : ''}</p></div>
	<p role="status" class="text-xs text-muted-foreground">{installs.label}</p>
	{#if installs.active || installs.starting}
		<progress max="100" value={percent ?? undefined} aria-label="安装下载进度" class="block h-1.5 w-full overflow-hidden rounded-full accent-primary [&::-webkit-progress-bar]:rounded-full [&::-webkit-progress-bar]:bg-chrome-hover [&::-webkit-progress-value]:rounded-full [&::-webkit-progress-value]:bg-primary"></progress>
		<div class="flex min-h-4 flex-wrap items-center justify-between gap-x-3 gap-y-1 text-[11px] tabular-nums text-muted-foreground">
				{#if totalBytes !== null}<span>{formatBytes(Math.min(downloadedBytes, totalBytes))} / {formatBytes(totalBytes)}</span>{:else if files !== null}<span>文件 {operation?.progress.completedFiles ?? 0} / {files}</span>{/if}
				{#if showSpeed}<span aria-label="下载速度">{installs.bytesPerSecond === null ? '—' : `${formatBytes(installs.bytesPerSecond)}/s`}</span>{/if}
			</div>
	{/if}
	{#if operation?.state === 'completed'}<p class="text-[11px] text-muted-foreground">{operation.result?.alreadyInstalled ? '已确认现有实例的安装文件。' : '实例已安装完成。'}</p>{/if}
	{#if operation?.state === 'completed' && library.loading}<p role="status" class="text-[11px] text-muted-foreground">正在刷新客户端列表…</p>{/if}
	{#if operation?.state === 'completed' && library.error}<div role="alert" class="rounded-lg bg-chrome-hover px-2.5 py-2 text-xs text-muted-foreground">安装已完成，但列表刷新失败。<button type="button" disabled={library.loading} onclick={() => void library.refresh()} class="ml-1 rounded px-1 text-primary underline underline-offset-2 disabled:opacity-40">重新刷新</button></div>{/if}
	{#if operation?.state === 'cancelled'}<p class="text-[11px] text-muted-foreground">安装任务已结束，已校验的缓存文件可供后续安装复用。</p>{/if}
	{#if installs.error || (operation?.error && !(operation.state === 'cancelled' && operation.error === 'operation cancelled'))}
		<div role="alert" class="rounded-lg bg-destructive/5 px-2.5 py-2 text-xs text-destructive">{installs.error ?? (operation?.state === 'cancelled' ? '取消过程返回了以下信息。' : '本次安装未完成。')}<details class="mt-1 text-[11px] text-muted-foreground"><summary class="cursor-default">错误详情</summary><p class="mt-1 max-h-28 overflow-y-auto select-text whitespace-pre-wrap wrap-break-word">{installs.errorDetail ?? operation?.error}</p></details></div>
	{/if}
	<div class="flex flex-wrap items-center justify-end gap-2">
		{#if operation?.state === 'completed' && operation.result}<a href={resolve('/clients')} onclick={() => { library.query = ''; installs.detailsOpen = false; onNavigate?.(); }} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-primary hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">查看客户端<ArrowUpRight size={12} aria-hidden="true" /></a>{/if}
		{#if operation?.state === 'failed' || operation?.state === 'cancelled'}<button type="button" disabled={!installs.canRetry} onclick={() => void installs.retry()} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><RotateCcw size={12} aria-hidden="true" />{installs.retrying || installs.syncing ? '确认中…' : '重新安装'}</button>{/if}
		{#if operation?.state === 'paused'}<button type="button" disabled={!installs.canResume} onclick={() => void installs.setPaused(false)} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><Play size={12} strokeWidth={1.75} aria-hidden="true" />继续下载</button>
		{:else if installs.canPause || installs.changingPause || operation?.state === 'pausing' || operation?.state === 'resuming'}<button type="button" disabled={!installs.canPause} onclick={() => void installs.setPaused(true)} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><Pause size={12} strokeWidth={1.75} aria-hidden="true" />{operation?.state === 'pausing' ? '暂停中…' : operation?.state === 'resuming' ? '继续中…' : '暂停下载'}</button>{/if}
		{#if installs.error || installs.uncertain}<button type="button" disabled={installs.syncing || installs.starting || installs.cancelling || installs.changingPause || installs.retrying} onclick={() => void installs.sync()} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring">{installs.syncing ? '查询中…' : '重新查询'}</button>{/if}
		{#if installs.active}<button type="button" disabled={!installs.canCancel || installs.syncing} onclick={() => void installs.cancel()} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><X size={12} strokeWidth={1.75} aria-hidden="true" />{installs.cancelling || operation?.cancelRequested ? '取消中…' : operation?.progress.stage === 'publishing' ? '正在保存…' : '取消安装'}</button>{/if}
	</div>
</div>
