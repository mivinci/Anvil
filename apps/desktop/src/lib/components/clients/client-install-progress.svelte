<script lang="ts">
	import { ArrowUpRight, Check, LoaderCircle, Pause, Play, RotateCcw, TriangleAlert, X } from '@lucide/svelte';
	import { formatBytes } from '../../download-progress';
	import { resolve } from '$app/paths';
	import { getClientLibraryState } from '../../client-library-state.svelte.js';
	let { onNavigate, onClose, closeLabel = '关闭' }: { onNavigate?: () => void; onClose?: () => void; closeLabel?: string } = $props();
	const library = getClientLibraryState();
	import { getClientInstallState } from '../../client-install-state.svelte.js';
	const installs = getClientInstallState();
	const operation = $derived(installs.operation);
	const files = $derived(operation?.progress.totalFiles ?? null);
	const totalBytes = $derived(operation?.progress.totalBytes ?? null);
	const downloadedBytes = $derived(operation?.progress.downloadedBytes ?? 0);
	const percent = $derived(totalBytes && operation ? Math.min(100, Math.max(0, downloadedBytes / totalBytes * 100)) : files && operation ? Math.min(100, Math.max(0, operation.progress.completedFiles / files * 100)) : null);
	const showSpeed = $derived(operation?.state === 'running' && operation.progress.stage === 'downloading' && operation.progress.receivedBytes > 0 && !operation.cancelRequested && !installs.error && !installs.syncing && !installs.changingPause);
	const completed = $derived(operation?.state === 'completed');
	const failed = $derived(!!installs.error || operation?.state === 'failed');
	const cancelled = $derived(operation?.state === 'cancelled');
	const paused = $derived(operation?.state === 'paused');
	const active = $derived(installs.active || installs.starting);
	const hasError = $derived(!!installs.error || !!(operation?.error && !(cancelled && operation.error === 'operation cancelled')));

	const statusLabel = $derived(completed
		? operation?.result?.alreadyInstalled ? '安装文件已确认' : '安装完成'
		: failed ? '安装失败'
			: cancelled ? '安装已取消'
				: paused ? '已暂停'
					: installs.label);
	const statusTone = $derived(completed || (!failed && !cancelled && !paused)
		? 'bg-primary/10 text-primary'
		: failed ? 'bg-destructive/10 text-destructive' : 'bg-chrome-hover text-muted-foreground');

	const completionStats = $derived.by(() => {
		if (!completed || !operation?.result) return [];
		const entries: { label: string; value: string }[] = [];
		if (operation.result.reusedFiles !== undefined) entries.push({ label: '复用缓存', value: operation.result.reusedFiles.toLocaleString('zh-CN') });
		if (operation.result.downloadedFiles !== undefined) entries.push({ label: '下载文件', value: operation.result.downloadedFiles.toLocaleString('zh-CN') });
		entries.push({ label: '网络接收', value: formatBytes(operation.progress.receivedBytes) });
		return entries;
	});
</script>

<div class="space-y-2.5">
	<div>
		<p class="truncate text-[13px] font-medium">{operation?.name ?? installs.intent?.name ?? 'Minecraft'}</p>
		<p class="mt-1 truncate text-[11px] text-muted-foreground">Minecraft {operation?.versionId === 'unknown' ? '待解析' : operation?.versionId ?? installs.intent?.versionId ?? '—'}{operation?.loader ? ` · Fabric ${operation.loader.version}` : ''}</p>
	</div>

	<div class="flex items-center gap-2">
		<span class={`grid size-5 shrink-0 place-items-center rounded-full ${statusTone}`}>
			{#if completed}<Check size={12} strokeWidth={2.5} aria-hidden="true" />
			{:else if failed}<TriangleAlert size={12} aria-hidden="true" />
			{:else if cancelled}<X size={12} aria-hidden="true" />
			{:else if paused}<Pause size={12} aria-hidden="true" />
			{:else}<LoaderCircle size={12} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{/if}
		</span>
		<p role="status" class={`min-w-0 flex-1 truncate text-xs ${failed ? 'text-destructive' : 'text-muted-foreground'}`}>{statusLabel}</p>
		{#if active && percent !== null}<span class="shrink-0 text-[11px] tabular-nums text-muted-foreground">{Math.round(percent)}%</span>{/if}
	</div>

	{#if active}
		<div>
			<progress max="100" value={percent ?? undefined} aria-label="安装准备进度" class="block h-1.5 w-full overflow-hidden rounded-full accent-primary [&::-webkit-progress-bar]:rounded-full [&::-webkit-progress-bar]:bg-chrome-hover [&::-webkit-progress-value]:rounded-full [&::-webkit-progress-value]:bg-primary"></progress>
			<div class="mt-1.5 flex min-h-4 flex-wrap items-center justify-between gap-x-3 gap-y-1 text-[11px] tabular-nums text-muted-foreground">
				{#if totalBytes !== null}<span>{operation?.progress.stage === 'java' ? '进度' : '已准备'} {formatBytes(Math.min(downloadedBytes, totalBytes))} / {formatBytes(totalBytes)}</span>{:else if files !== null}<span>文件 {operation?.progress.completedFiles ?? 0} / {files}</span>{/if}
				{#if showSpeed}<span aria-label="下载速度">{installs.bytesPerSecond === null ? '—' : `${formatBytes(installs.bytesPerSecond)}/s`}</span>{/if}
			</div>
		</div>
	{/if}

	{#if completionStats.length}
		<div class="flex gap-2 rounded-lg bg-chrome-hover px-2.5 py-2">
			{#each completionStats as stat (stat.label)}
				<div class="min-w-0 flex-1">
					<p class="truncate text-[13px] font-medium tabular-nums text-foreground">{stat.value}</p>
					<p class="mt-0.5 truncate text-[10px] text-muted-foreground">{stat.label}</p>
				</div>
			{/each}
		</div>
	{/if}

	{#if completed && library.loading}<p role="status" class="text-[11px] text-muted-foreground">正在刷新客户端列表…</p>{/if}
	{#if completed && library.error}<div role="alert" class="flex flex-wrap items-center gap-x-2 gap-y-1 rounded-lg bg-chrome-hover px-2.5 py-2 text-[11px] text-muted-foreground"><span>安装已完成，但列表刷新失败。</span><button type="button" disabled={library.loading} onclick={() => void library.refresh()} class="rounded px-1 text-primary underline underline-offset-2 disabled:opacity-40">重新刷新</button></div>{/if}
	{#if operation?.importRequestId && (failed || cancelled) && !installs.importFile}<p class="text-[11px] text-muted-foreground">重新导入需要在客户端页面再次选择原整合包文件。</p>{/if}
	{#if cancelled}<p class="text-[11px] leading-5 text-muted-foreground">安装任务已结束，已校验的缓存文件可供后续安装复用。</p>{/if}
	{#if hasError}
		<div role="alert" class="rounded-lg bg-destructive/5 px-2.5 py-2 text-xs text-destructive">{installs.error ?? (cancelled ? '取消过程返回了以下信息。' : operation?.importRequestId ? operation.error ?? '整合包导入未完成。' : '本次安装未完成。')}<details class="mt-1 text-[11px] text-muted-foreground"><summary class="cursor-default">错误详情</summary><p class="mt-1 max-h-28 overflow-y-auto select-text whitespace-pre-wrap wrap-break-word">{installs.errorDetail ?? operation?.error}</p></details></div>
	{/if}

	<div class="flex flex-wrap items-center justify-end gap-1.5">
		{#if installs.active || installs.starting && installs.importing}<button type="button" disabled={!installs.canCancel || installs.syncing} onclick={() => void installs.cancel()} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><X size={12} strokeWidth={1.75} aria-hidden="true" />{installs.cancelling || operation?.cancelRequested ? '取消中…' : operation?.progress.stage === 'publishing' ? '正在保存…' : '取消安装'}</button>{/if}
		{#if operation?.state === 'failed' || operation?.state === 'cancelled'}<button type="button" disabled={!installs.canRetry} onclick={() => void installs.retry()} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><RotateCcw size={12} aria-hidden="true" />{installs.retrying || installs.syncing ? '确认中…' : '重新安装'}</button>{/if}
		{#if installs.error || installs.uncertain}<button type="button" disabled={installs.syncing || installs.starting || installs.cancelling || installs.changingPause || installs.retrying} onclick={() => void installs.sync()} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring">{installs.syncing ? '查询中…' : '重新查询'}</button>{/if}
		{#if paused}<button type="button" disabled={!installs.canResume} onclick={() => void installs.setPaused(false)} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><Play size={12} strokeWidth={1.75} aria-hidden="true" />继续下载</button>
		{:else if installs.canPause || installs.changingPause || operation?.state === 'pausing' || operation?.state === 'resuming'}<button type="button" disabled={!installs.canPause} onclick={() => void installs.setPaused(true)} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><Pause size={12} strokeWidth={1.75} aria-hidden="true" />{operation?.state === 'pausing' ? '暂停中…' : operation?.state === 'resuming' ? '继续中…' : '暂停下载'}</button>{/if}
		{#if completed && operation?.result}<a href={resolve('/clients')} onclick={() => { library.query = ''; installs.detailsOpen = false; onNavigate?.(); }} class="inline-flex h-8 items-center justify-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground hover:bg-primary/90 focus-visible:outline-2 focus-visible:outline-ring">查看客户端<ArrowUpRight size={12} aria-hidden="true" /></a>{/if}
		{#if onClose}<button type="button" onclick={onClose} class="inline-flex h-8 items-center justify-center rounded-lg px-3 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">{closeLabel}</button>{/if}
	</div>
</div>
