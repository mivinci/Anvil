<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { tick } from 'svelte';
	import { page } from '$app/state';
	import ClientCrashReports from '#lib/components/clients/client-crash-reports.svelte';
	import { Copy, RefreshCw } from '@lucide/svelte';
	import { getClientDetailState } from '#lib/client-detail-state.svelte.js';
	import { getClientGameLog, getClientConsoleLog, type ClientConsoleLog, type ClientGameLog } from '#lib/runtime.js';
	const detail = getClientDetailState();
	let viewport = $state<HTMLDivElement>();
	let source = $state<'game' | 'console' | 'crashes'>(page.url.searchParams.get('source') === 'console' ? 'console' : page.url.searchParams.get('source') === 'crashes' ? 'crashes' : 'game');
	let snapshot = $state<ClientGameLog | ClientConsoleLog | null>(null);
	let loading = $state(false);
	let errorValue = $state<string | (() => string | null) | null>(null);
	const error = $derived(typeof errorValue === 'function' ? errorValue() : errorValue);
	let copyStatusValue = $state<string | (() => string | null) | null>(null);
	const copyStatus = $derived(typeof copyStatusValue === 'function' ? copyStatusValue() : copyStatusValue);
	let updated = $state<number | null>(null);
	let request: AbortController | null = null;
	let currentName = '';
	let currentSource = '';
	const isConsole = $derived(source === 'console');
	const readFailed = $derived(snapshot && 'readFailed' in snapshot && snapshot.readFailed);
	async function refresh(name: string, kind: 'game' | 'console' | 'crashes') {
		request?.abort();
		const controller = new AbortController(); request = controller;
		loading = true; errorValue = null; copyStatusValue = null;
		try {
			const value = await (kind === 'console' ? getClientConsoleLog : getClientGameLog)(name, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (controller.signal.aborted) return;
			snapshot = value; updated = Date.now();
			await tick();
			if (!controller.signal.aborted && viewport) viewport.scrollTop = viewport.scrollHeight;
		} catch { if (!controller.signal.aborted) errorValue = () => (localeState.messages.instance.detail.logReadFailed); }
		finally { if (request === controller) { request = null; loading = false; } }
	}
	$effect(() => {
		const name = detail.client?.name;
		if (name && source !== 'crashes') {
			if (name !== currentName || source !== currentSource) { currentName = name; currentSource = source; snapshot = null; updated = null; }
			void refresh(name, source);
		}
		return () => { request?.abort(); request = null; };
	});
	async function copy() {
		const text = snapshot?.text;
		if (!text) return;
		try { await navigator.clipboard.writeText(text); copyStatusValue = () => (localeState.messages.instance.detail.logCopied); }
		catch { copyStatusValue = () => (localeState.messages.instance.detail.copyUnavailable); }
	}
</script>
<svelte:head><title>{localeState.messages.common.instanceTitle(detail.client?.name ?? localeState.messages.nav.instance, localeState.messages.instance.logs)}</title></svelte:head>
<section aria-labelledby="game-log-title" class="flex min-h-0 flex-1 flex-col gap-3 px-5 py-5 lg:px-6">
	<div class="flex items-center justify-between gap-3">
		<div class="flex min-w-0 items-center gap-2"><h1 id="game-log-title" class="text-base font-semibold tracking-tight">{localeState.messages.instance.detail.gameLog}</h1><span class="text-xs text-muted-foreground">{source === 'crashes' ? 'crash-reports' : isConsole ? 'stdout / stderr' : 'latest.log'}</span></div>
		{#if source !== 'crashes'}<div class="flex items-center gap-1">
			<button type="button" disabled={!snapshot?.text} onclick={() => void copy()} aria-label={localeState.messages.instance.detail.copyLog} title={localeState.messages.instance.detail.copyLog} class="grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Copy size={14} aria-hidden="true" /></button>
			<button type="button" disabled={loading || !detail.client} onclick={() => { if (detail.client) void refresh(detail.client.name, source); }} aria-label={localeState.messages.instance.detail.refreshLogs} title={localeState.messages.instance.detail.refreshLogs} class="grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={14} class={loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button>
		</div>{/if}
	</div>
	<div class="flex items-center gap-1 self-start rounded-lg bg-chrome-hover p-1" aria-label={localeState.messages.instance.detail.logSource}>
		{#each [{ value: 'game', label: localeState.messages.instance.detail.gameLog }, { value: 'console', label: localeState.messages.instance.detail.javaOutput }, { value: 'crashes', label: localeState.messages.instance.detail.crashReports }] as option}
			<button type="button" aria-pressed={source === option.value} onclick={() => source = option.value as 'game' | 'console' | 'crashes'} class={`rounded-md px-2.5 py-1 text-xs transition-colors focus-visible:outline-2 focus-visible:outline-ring ${source === option.value ? 'bg-chrome-selected font-medium text-accent-foreground' : 'text-muted-foreground hover:text-foreground'}`}>{option.label}</button>
		{/each}
	</div>
	{#if source === 'crashes'}
		<ClientCrashReports />
	{:else}
	{#if readFailed}<p role="status" class="text-xs text-muted-foreground">{localeState.messages.instance.detail.consoleReadFailed}</p>{/if}
	{#if error}<p role="alert" class="text-xs text-destructive">{error}{snapshot ? localeState.messages.instance.keepContent : ''}</p>{/if}
	<div bind:this={viewport} class="min-h-0 flex-1 overflow-auto overscroll-contain rounded-lg bg-card">
		{#if snapshot?.text}<pre class="min-w-full w-max select-text p-3 font-mono text-xs leading-5 text-foreground"><code>{snapshot.text}</code></pre>
		{:else}<div role="status" class="flex h-full min-h-32 items-center justify-center px-4 text-body text-muted-foreground">{loading ? localeState.messages.instance.detail.loadingLog : snapshot?.exists ? (snapshot.totalBytes ? localeState.messages.instance.detail.noCompleteLines : isConsole ? localeState.messages.instance.detail.noConsoleLaunch : localeState.messages.instance.detail.logEmpty) : snapshot ? (isConsole ? localeState.messages.instance.detail.noConsoleSession : localeState.messages.instance.detail.noGameLog) : localeState.messages.instance.detail.noLog}</div>{/if}
	</div>
	<div class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
		<span>{snapshot?.truncated ? localeState.messages.instance.detail.outputTruncated : isConsole ? localeState.messages.instance.detail.recentLaunch : localeState.messages.instance.detail.gameOwnLog}{updated ? ` · ${localeState.messages.instance.updatedTime(new Intl.DateTimeFormat(localeState.current, { hour: '2-digit', minute: '2-digit', second: '2-digit' }).format(updated))}` : ''}</span>
		<span role="status">{copyStatus ?? (isConsole ? localeState.messages.instance.detail.consoleLimit : localeState.messages.instance.detail.refreshHint)}</span>
	</div>
	{/if}
</section>
