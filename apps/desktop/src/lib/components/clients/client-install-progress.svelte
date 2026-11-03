<script lang="ts">
	import { runtimeErrorMessage } from '#lib/runtime-errors.ts';
	import { localeState } from '#lib/locale.svelte.ts';
	import { ArrowUpRight, Check, LoaderCircle, Pause, Play, RotateCcw, TriangleAlert, X } from '@lucide/svelte';
	import { formatBytes as formatDownloadBytes } from '../../download-progress';
	const formatBytes = (value: number) => formatDownloadBytes(value, localeState.current);
	import { resolve } from '$app/paths';
	import { getClientLibraryState } from '../../client-library-state.svelte.js';
	let { onNavigate, onClose, closeLabel }: { onNavigate?: () => void; onClose?: () => void; closeLabel?: string } = $props();
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
	const hasError = $derived(!!installs.error || !!(operation?.error && !(cancelled && operation.errorCode === 'cancelled')));

	const statusLabel = $derived(completed
		? operation?.result?.alreadyInstalled ? localeState.messages.clients.flow.filesConfirmed : localeState.messages.clients.flow.installComplete
		: failed ? localeState.messages.clients.flow.installFailed
			: cancelled ? localeState.messages.clients.flow.installCancelled
				: paused ? localeState.messages.clients.flow.paused
					: installs.label);
	const statusTone = $derived(completed || (!failed && !cancelled && !paused)
		? 'bg-primary/10 text-primary'
		: failed ? 'bg-destructive/10 text-destructive' : 'bg-chrome-hover text-muted-foreground');

	const completionStats = $derived.by(() => {
		if (!completed || !operation?.result) return [];
		const entries: { label: string; value: string }[] = [];
		if (operation.result.reusedFiles !== undefined) entries.push({ label: localeState.messages.clients.flow.reused, value: operation.result.reusedFiles.toLocaleString(localeState.current) });
		if (operation.result.downloadedFiles !== undefined) entries.push({ label: localeState.messages.clients.flow.downloadedFiles, value: operation.result.downloadedFiles.toLocaleString(localeState.current) });
		entries.push({ label: localeState.messages.clients.flow.received, value: formatBytes(operation.progress.receivedBytes) });
		return entries;
	});
</script>

<div class="space-y-2.5">
	<div>
		<p class="truncate text-[13px] font-medium">{operation?.name ?? installs.intent?.name ?? 'Minecraft'}</p>
		<p class="mt-1 truncate text-[11px] text-muted-foreground">Minecraft {(operation?.versionId ?? installs.intent?.versionId) === 'unknown' ? localeState.messages.clients.flow.pendingParsing : operation?.versionId ?? installs.intent?.versionId ?? '—'}{operation?.loader ? ` · Fabric ${operation.loader.version}` : ''}</p>
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
			<progress max="100" value={percent ?? undefined} aria-label={localeState.messages.clients.flow.installProgress} class="block h-1.5 w-full overflow-hidden rounded-full accent-primary [&::-webkit-progress-bar]:rounded-full [&::-webkit-progress-bar]:bg-chrome-hover [&::-webkit-progress-value]:rounded-full [&::-webkit-progress-value]:bg-primary"></progress>
			<div class="mt-1.5 flex min-h-4 flex-wrap items-center justify-between gap-x-3 gap-y-1 text-[11px] tabular-nums text-muted-foreground">
				{#if totalBytes !== null}<span>{operation?.progress.stage === 'java' ? localeState.messages.clients.flow.progress : localeState.messages.clients.flow.prepared} {formatBytes(Math.min(downloadedBytes, totalBytes))} / {formatBytes(totalBytes)}</span>{:else if files !== null}<span>{localeState.messages.clients.flow.files} {operation?.progress.completedFiles ?? 0} / {files}</span>{/if}
				{#if showSpeed}<span aria-label={localeState.messages.clients.flow.speed}>{installs.bytesPerSecond === null ? '—' : `${formatBytes(installs.bytesPerSecond)}/s`}</span>{/if}
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

	{#if completed && library.loading}<p role="status" class="text-[11px] text-muted-foreground">{localeState.messages.clients.flow.refreshingClients}</p>{/if}
	{#if completed && library.error}<div role="alert" class="flex flex-wrap items-center gap-x-2 gap-y-1 rounded-lg bg-chrome-hover px-2.5 py-2 text-[11px] text-muted-foreground"><span>{localeState.messages.clients.flow.installedRefreshFailed}</span><button type="button" disabled={library.loading} onclick={() => void library.refresh()} class="rounded px-1 text-primary underline underline-offset-2 disabled:opacity-40">{localeState.messages.clients.flow.refreshAgain}</button></div>{/if}
	{#if operation?.importRequestId && (failed || cancelled) && !installs.importFile}<p class="text-[11px] text-muted-foreground">{localeState.messages.clients.flow.reselectPack}</p>{/if}
	{#if cancelled}<p class="text-[11px] leading-5 text-muted-foreground">{localeState.messages.clients.flow.cacheReusable}</p>{/if}
	{#if hasError}
		<div role="alert" class="rounded-lg bg-destructive/5 px-2.5 py-2 text-xs text-destructive">{installs.error ?? runtimeErrorMessage(operation?.errorCode, localeState.current, cancelled ? localeState.messages.clients.flow.cancelInfo : operation?.importRequestId ? localeState.messages.clients.flow.importFailed : localeState.messages.clients.flow.installIncomplete)}<details class="mt-1 text-[11px] text-muted-foreground"><summary class="cursor-default">{localeState.messages.clients.flow.errorDetails}</summary><p class="mt-1 max-h-28 overflow-y-auto select-text whitespace-pre-wrap wrap-break-word">{installs.errorDetail ?? operation?.error}</p></details></div>
	{/if}

	<div class="flex flex-wrap items-center justify-end gap-1.5">
		{#if installs.active || installs.starting && installs.importing}<button type="button" disabled={!installs.canCancel || installs.syncing} onclick={() => void installs.cancel()} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><X size={12} strokeWidth={1.75} aria-hidden="true" />{installs.cancelling || operation?.cancelRequested ? localeState.messages.clients.flow.cancelWorking : operation?.progress.stage === 'publishing' ? localeState.messages.clients.flow.saving : localeState.messages.clients.flow.cancelInstall}</button>{/if}
		{#if operation?.state === 'failed' || operation?.state === 'cancelled'}<button type="button" disabled={!installs.canRetry} onclick={() => void installs.retry()} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><RotateCcw size={12} aria-hidden="true" />{installs.retrying || installs.syncing ? localeState.messages.clients.flow.confirmWorking : localeState.messages.clients.flow.reinstall}</button>{/if}
		{#if installs.error || installs.uncertain}<button type="button" disabled={installs.syncing || installs.starting || installs.cancelling || installs.changingPause || installs.retrying} onclick={() => void installs.sync()} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring">{installs.syncing ? localeState.messages.clients.flow.queryWorking : localeState.messages.clients.flow.queryAgain}</button>{/if}
		{#if paused}<button type="button" disabled={!installs.canResume} onclick={() => void installs.setPaused(false)} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-primary hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><Play size={12} strokeWidth={1.75} aria-hidden="true" />{localeState.messages.clients.flow.resumeDownload}</button>
		{:else if installs.canPause || installs.changingPause || operation?.state === 'pausing' || operation?.state === 'resuming'}<button type="button" disabled={!installs.canPause} onclick={() => void installs.setPaused(true)} class="inline-flex h-7 items-center justify-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring"><Pause size={12} strokeWidth={1.75} aria-hidden="true" />{operation?.state === 'pausing' ? localeState.messages.clients.flow.pausing : operation?.state === 'resuming' ? localeState.messages.clients.flow.resuming : localeState.messages.clients.flow.pauseDownload}</button>{/if}
		{#if completed && operation?.result}<a href={resolve('/clients')} onclick={() => { library.query = ''; installs.detailsOpen = false; onNavigate?.(); }} class="inline-flex h-8 items-center justify-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground hover:bg-primary/90 focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.clients.flow.viewClients}<ArrowUpRight size={12} aria-hidden="true" /></a>{/if}
		{#if onClose}<button type="button" onclick={onClose} class="inline-flex h-8 items-center justify-center rounded-lg px-3 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">{closeLabel ?? localeState.messages.common.close}</button>{/if}
	</div>
</div>
