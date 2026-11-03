<script lang="ts">
	import { requestErrorMessage } from '#lib/runtime-errors.ts';
	import { localeState } from '#lib/locale.svelte.ts';
	import { untrack } from 'svelte';
	import { Copy, RefreshCw } from '@lucide/svelte';
	import { getClientDetailState } from '../../client-detail-state.svelte.js';
	import { listClientCrashReports, getClientCrashReport, RuntimeRequestError, type ClientCrashReports, type ClientCrashReport, type ClientCrashReportEntry } from '../../runtime';
	const detail = getClientDetailState();
	let catalog = $state<ClientCrashReports | null>(null);
	let report = $state<ClientCrashReport | null>(null);
	let selected = $state<string | null>(null);
	let listing = $state(false);
	let reading = $state(false);
	let listErrorValue = $state<string | (() => string | null) | null>(null);
	const listError = $derived(typeof listErrorValue === 'function' ? listErrorValue() : listErrorValue);
	let readErrorValue = $state<string | (() => string | null) | null>(null);
	const readError = $derived(typeof readErrorValue === 'function' ? readErrorValue() : readErrorValue);
	let copiedValue = $state<string | (() => string | null) | null>(null);
	const copied = $derived(typeof copiedValue === 'function' ? copiedValue() : copiedValue);
	let listRequest: AbortController | null = null;
	let readRequest: AbortController | null = null;
	let currentName = '';
	let technicalError = $state<string | null>(null);
	const size = (bytes: number) => bytes < 1024 ? `${new Intl.NumberFormat(localeState.current).format(bytes)} B` : bytes < 1024 * 1024 ? `${new Intl.NumberFormat(localeState.current, { maximumFractionDigits: 0 }).format(bytes / 1024)} KiB` : `${new Intl.NumberFormat(localeState.current, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(bytes / 1024 / 1024)} MiB`;
	async function select(name: string, entry: ClientCrashReportEntry) {
		readRequest?.abort(); readRequest = null;
		selected = entry.name; report = null; readErrorValue = null; copiedValue = null; reading = false;
		if (entry.size > (catalog?.maxReportBytes ?? 2 * 1024 * 1024)) { readErrorValue = () => (localeState.messages.instance.detail.reportTooLarge); return; }
		const controller = new AbortController(); readRequest = controller; reading = true;
		try {
			const value = await getClientCrashReport(name, entry.name, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (!controller.signal.aborted) report = value;
		} catch (error) { if (!controller.signal.aborted) { technicalError = error instanceof Error ? error.message : String(error); readErrorValue = () => requestErrorMessage(error, localeState.current, localeState.messages.instance.detail.reportReadFailed); } }
		finally { if (readRequest === controller) { readRequest = null; reading = false; } }
	}
	async function refresh(name: string) {
		listRequest?.abort(); readRequest?.abort(); readRequest = null; reading = false;
		const controller = new AbortController(); listRequest = controller; listing = true; listErrorValue = null;
		try {
			const value = await listClientCrashReports(name, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (controller.signal.aborted) return;
			catalog = value;
			const entry = value.reports.find(item => item.name === selected) ?? value.reports[0];
			if (entry) void select(name, entry);
			else { selected = null; report = null; readErrorValue = null; copiedValue = null; }
		} catch (error) { if (!controller.signal.aborted) { technicalError = error instanceof Error ? error.message : String(error); listErrorValue = () => requestErrorMessage(error, localeState.current, localeState.messages.instance.detail.reportReadFailed); } }
		finally { if (listRequest === controller) { listRequest = null; listing = false; } }
	}
	$effect(() => {
		const name = detail.client?.name;
		if (name) untrack(() => {
			if (name !== currentName) { currentName = name; catalog = null; report = null; selected = null; copiedValue = null; readErrorValue = null; }
			void refresh(name);
		});
		return () => { listRequest?.abort(); readRequest?.abort(); listRequest = null; readRequest = null; };
	});
	async function copy() {
		const value = report;
		if (!value) return;
		try { await navigator.clipboard.writeText(value.text); if (report === value) copiedValue = () => (localeState.messages.instance.detail.reportCopied); }
		catch { if (report === value) copiedValue = () => (localeState.messages.instance.detail.copyUnavailable); }
	}
</script>
<div class="flex min-h-0 flex-1 flex-col gap-2">
	<div class="flex min-h-7 items-center justify-between gap-2 text-[11px] text-muted-foreground">
		<span>{catalog ? localeState.messages.instance.reportsCount(catalog.reports.length, catalog.truncated) : 'game/crash-reports'}</span>
		<div class="flex items-center gap-1">
			<button type="button" disabled={!report?.text} onclick={() => void copy()} aria-label={localeState.messages.instance.detail.copyCrash} title={localeState.messages.instance.detail.copyReport} class="grid size-7 place-items-center rounded-md hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Copy size={14} aria-hidden="true" /></button>
			<button type="button" disabled={listing || !detail.client} onclick={() => { if (detail.client) void refresh(detail.client.name); }} aria-label={localeState.messages.instance.detail.refreshCrashes} title={localeState.messages.instance.detail.refreshCrashes} class="grid size-7 place-items-center rounded-md hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={14} class={listing ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button>
		</div>
	</div>
	{#if (listError || readError) && technicalError}<details class="text-xs text-muted-foreground"><summary>{localeState.messages.errors.technicalDetails}</summary><p class="whitespace-pre-wrap break-words">{technicalError}</p></details>{/if}
	{#if listError}<p role="alert" class="text-[11px] text-destructive">{listError}{catalog ? localeState.messages.instance.keepList : ''}</p>{/if}
	<div class="flex min-h-0 flex-1 overflow-hidden rounded-lg bg-card">
		{#if catalog?.reports.length}
			<nav aria-label={localeState.messages.instance.detail.crashList} class="w-40 shrink-0 space-y-1 overflow-y-auto overscroll-contain p-1.5 lg:w-48">
				{#each catalog.reports as entry (entry.name)}
					<button type="button" disabled={listing} aria-pressed={selected === entry.name} onclick={() => { if (detail.client) void select(detail.client.name, entry); }} title={entry.name} class={`block w-full rounded-md px-2 py-2 text-left focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-50 ${selected === entry.name ? 'bg-chrome-selected text-accent-foreground' : 'hover:bg-chrome-hover'}`}>
						<span class="block truncate text-[12px] leading-4">{entry.name}</span>
						<span class="mt-1 block truncate text-[11px] leading-4 text-muted-foreground">{entry.modifiedAt !== null ? new Date(entry.modifiedAt).toLocaleString(localeState.current, { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : localeState.messages.instance.detail.unknownTime} · {size(entry.size)}</span>
					</button>
				{/each}
			</nav>
			<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
				<p class="shrink-0 truncate border-b border-border px-3 py-2 text-[11px] text-muted-foreground" title={selected ?? undefined}>{selected}</p>
				<div class="min-h-0 flex-1 overflow-auto overscroll-contain">
					{#if report?.text}<pre class="w-max min-w-full select-text p-3 font-mono text-[12px] leading-5"><code>{report.text}</code></pre>
					{:else}<div role={readError ? 'alert' : 'status'} class={`flex h-full min-h-32 items-center justify-center px-4 text-[13px] ${readError ? 'text-destructive' : 'text-muted-foreground'}`}><span>{reading ? localeState.messages.instance.detail.loadingReport : readError ?? (report ? localeState.messages.instance.detail.reportEmpty : localeState.messages.instance.detail.chooseReport)}{#if readError && detail.client}<button type="button" class="ml-2 text-primary" onclick={() => { const entry = catalog?.reports.find(item => item.name === selected); if (entry && detail.client) void select(detail.client.name, entry); }}>{localeState.messages.instance.detail.retry}</button>{/if}</span></div>{/if}
				</div>
			</div>
		{:else}<div role="status" class="flex min-h-32 flex-1 items-center justify-center px-4 text-[13px] text-muted-foreground">{listing ? localeState.messages.instance.detail.loadingReports : catalog ? localeState.messages.instance.detail.noCrashes : localeState.messages.instance.detail.noReportList}</div>{/if}
	</div>
	<p role="status" class="text-[11px] text-muted-foreground">{copied ?? localeState.messages.instance.detail.reportLimit}</p>
</div>
