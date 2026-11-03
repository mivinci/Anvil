<script lang="ts">
	import { localeState } from '#lib/locale.svelte.js';
	import { shellUpgradeRequired, shellUpgradeMessage } from '../../shell-compatibility';
	import { onMount } from 'svelte';
	import { isTauri } from '@tauri-apps/api/core';
	import { ExternalLink, FolderOpen, RefreshCw } from '@lucide/svelte';
	import { shell, type AppInfo, type RuntimeStatus, type LoggingStatus, type UpdateStatus } from '../../shell';
	import { getRuntimeFileLoggingStatus } from '../../runtime';
	let logging = $state<LoggingStatus | null>(null);
	let runtimeFileLogging = $state<boolean | null>(null);
	let logsLoading = $state(false);
	let logsOpening = $state(false);
	let logsErrorValue = $state<string | (() => string | null) | null>(null);
	const logsError = $derived(typeof logsErrorValue === 'function' ? logsErrorValue() : logsErrorValue);
	async function loadLogs() {
		if (!isTauri() || logsLoading) return;
		logsLoading = true;
		logsErrorValue = null;
		const [native, health] = await Promise.allSettled([shell.getLoggingStatus(), getRuntimeFileLoggingStatus()]);
		logging = native.status === 'fulfilled' ? native.value : null;
		runtimeFileLogging = health.status === 'fulfilled' ? health.value : null;
		if (native.status === 'rejected') logsErrorValue = () => (localeState.messages.settings.flow.logsReadFailed);
		logsLoading = false;
	}
	async function openLogs() {
		if (logsOpening) return;
		logsOpening = true;
		logsErrorValue = null;
		try { await shell.openLogDirectory(); }
		catch { logsErrorValue = () => (localeState.messages.settings.flow.logsOpenFailed); }
		finally { logsOpening = false; }
	}
	function logState(available: boolean | null | undefined) {
		if (!isTauri()) return localeState.messages.settings.flow.desktopOnly;
		if (logsLoading) return localeState.messages.settings.flow.reading;
		return available === true ? localeState.messages.settings.flow.fileLoggingEnabled : available === false ? localeState.messages.settings.flow.fileLoggingUnavailable : localeState.messages.settings.flow.statusUnavailable;
	}
	let info = $state<AppInfo | null>(null);
	let runtime = $state<RuntimeStatus | null>(null);
	let updates = $state<UpdateStatus | null>(null);
	let checkingUpdates = $state(false);
	let updateErrorValue = $state<string | (() => string | null) | null>(null);
	const updateError = $derived(typeof updateErrorValue === 'function' ? updateErrorValue() : updateErrorValue);
	let updateTimer: ReturnType<typeof setTimeout> | undefined;
	let errorValue = $state<string | (() => string | null) | null>(null);
	const error = $derived(typeof errorValue === 'function' ? errorValue() : errorValue);
	let loading = $state(true);
	async function load() {
		loading = true;
		errorValue = null;
		if (!isTauri()) { loading = false; return; }
		try { [info, runtime, updates] = await Promise.all([shell.getAppInfo(), shell.getRuntimeStatus(), shell.getUpdateStatus()]); }
		catch { errorValue = () => (localeState.messages.settings.flow.versionReadFailed); }
		finally { loading = false; }
	}
	async function refreshUpdates(attempt = 0) {
		try {
			updates = await shell.getUpdateStatus();
			const appBusy = ['checking', 'downloading', 'verifying'].includes(updates.app.state);
			if (updates.shell.state === 'checking' || appBusy || attempt < 2) {
				updateTimer = setTimeout(() => void refreshUpdates(attempt + 1), 800);
			} else checkingUpdates = false;
		} catch {
			updateErrorValue = () => (localeState.messages.settings.flow.updateReadFailed);
			checkingUpdates = false;
		}
	}
	async function checkUpdates() {
		if (!isTauri() || checkingUpdates) return;
		checkingUpdates = true;
		updateErrorValue = null;
		try {
			await shell.checkForUpdates();
			await refreshUpdates();
		} catch {
			updateErrorValue = () => (localeState.messages.settings.flow.updateCheckFailed);
			checkingUpdates = false;
		}
	}
	async function openShellRelease() {
		const url = updates?.shell.state === 'available' && updates.shell.releaseUrl
			? updates.shell.releaseUrl : shellUpgradeRequired(updates?.app) ? 'https://github.com/mivinci/Anvil/releases' : null;
		if (!url) return;
		try { await shell.openExternalUrl(url); }
		catch { updateErrorValue = () => (localeState.messages.settings.flow.releaseOpenFailed); }
	}
	onMount(() => {
		void load();
		void loadLogs();
		return () => { if (updateTimer) clearTimeout(updateTimer); };
	});
	async function openExternal(event: MouseEvent, url: string) {
		if (!isTauri()) return;
		event.preventDefault();
		try { await shell.openExternalUrl(url); }
		catch { errorValue = () => (localeState.messages.settings.flow.browserFailed); }
	}
</script>

<div class="space-y-5">
	<div class="flex items-center gap-4">
		<img src="/logo.png" alt="" width="64" height="64" class="size-16 shrink-0 rounded-xl object-cover" />
		<div class="min-w-0">
			<p class="text-xl font-semibold tracking-tight">Anvil</p>
			<p class="mt-1 text-xs leading-5 text-muted-foreground">{localeState.messages.settings.flow.productDescription}</p>
		</div>
	</div>
	<section aria-label={localeState.messages.settings.flow.versions} class="space-y-2">
		<div class="flex items-center justify-between gap-3">
			<h3 class="text-body font-medium">{localeState.messages.settings.flow.versions}</h3>
			<button type="button" aria-label={localeState.messages.settings.flow.checkUpdates} title={localeState.messages.settings.flow.checkUpdates} disabled={!isTauri() || checkingUpdates} onclick={() => void checkUpdates()} class="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-chrome-hover hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={13} class={checkingUpdates ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" />{checkingUpdates ? localeState.messages.settings.flow.checking : localeState.messages.settings.flow.checkUpdates}</button>
		</div>
		<div class="divide-y divide-border rounded-lg bg-card px-3.5 text-body">
			{#each [[localeState.messages.settings.flow.shellVersion, info?.shellVersion], [localeState.messages.settings.flow.shellApi, info?.shellApiVersion], [localeState.messages.settings.flow.appVersion, info?.appVersion], [localeState.messages.settings.flow.runtimeVersion, runtime?.runtimeVersion], [localeState.messages.settings.flow.runtimeApi, runtime?.apiVersion], [localeState.messages.settings.flow.platform, info?.targetPlatform]] as [label, value]}
				<div class="flex min-h-10 items-center justify-between gap-4 py-2"><span>{label}</span><span class="select-text text-right text-muted-foreground">{loading ? localeState.messages.settings.flow.reading : value ?? (isTauri() ? localeState.messages.settings.flow.unavailable : localeState.messages.settings.flow.desktopOnly)}</span></div>
			{/each}
		</div>
		{#if updates && shellUpgradeRequired(updates.app)}
			<div role="status" class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-primary/20 bg-primary/5 px-3.5 py-2.5 text-xs">
				<p class="min-w-0 flex-1 leading-5">{shellUpgradeMessage(updates.app, localeState.current)}</p>
				<button type="button" onclick={() => void openShellRelease()} class="flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-primary hover:bg-primary/10 focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.settings.flow.viewInstallers}<ExternalLink size={12} aria-hidden="true" /></button>
			</div>
		{:else if updates?.shell.state === 'available'}
			<div class="flex items-center justify-between gap-3 rounded-lg border border-primary/20 bg-primary/5 px-3.5 py-2.5 text-xs">
				<span>{localeState.messages.settings.shellAvailable(updates.shell.availableVersion)}</span>
				<button type="button" onclick={() => void openShellRelease()} class="flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-primary hover:bg-primary/10 focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.settings.flow.openRelease}<ExternalLink size={12} aria-hidden="true" /></button>
			</div>
		{/if}
		{#if updateError}<p role="alert" class="text-xs text-destructive">{updateError}</p>{/if}
	</section>
	<section aria-label={localeState.messages.settings.flow.logs} class="space-y-2">
		<div class="flex items-center justify-between gap-3">
			<h3 class="text-body font-medium">{localeState.messages.settings.flow.logs}</h3>
			<button type="button" aria-label={localeState.messages.settings.flow.refreshLogs} title={localeState.messages.settings.flow.refreshLogs} disabled={!isTauri() || logsLoading} onclick={() => void loadLogs()} class="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={13} aria-hidden="true" /></button>
		</div>
		<div class="divide-y divide-border rounded-lg bg-card px-3.5 text-body">
			<div class="flex items-center justify-between gap-3 py-2">
				<div class="min-w-0"><p>{localeState.messages.settings.flow.logDirectory}</p><p class="mt-1 select-text break-all text-xs leading-4 text-muted-foreground">{logging?.directory ?? (logsLoading ? localeState.messages.settings.flow.reading : isTauri() ? localeState.messages.settings.flow.directoryUnavailable : localeState.messages.settings.flow.desktopOnly)}</p></div>
				<button type="button" disabled={!logging?.directory || logsOpening} onclick={() => void openLogs()} class="flex shrink-0 items-center gap-1.5 rounded-md px-2 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-chrome-hover hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><FolderOpen size={14} aria-hidden="true" />{logsOpening ? localeState.messages.settings.flow.opening : localeState.messages.settings.flow.openDirectory}</button>
			</div>
			{#each [['Shell', logging?.fileLoggingAvailable], ['Runtime', runtimeFileLogging]] as [label, available]}
				<div class="flex min-h-9 items-center justify-between gap-3 py-2"><span>{label}</span><span class="text-right text-xs text-muted-foreground" class:text-destructive={available === false}>{logState(available as boolean | null | undefined)}</span></div>
			{/each}
		</div>
		<p class="text-xs leading-4 text-muted-foreground">{localeState.messages.settings.flow.logRotation}</p>
		{#if logsError}<p role="alert" class="text-xs text-destructive">{logsError}</p>{/if}
	</section>
	<div class="divide-y divide-border rounded-lg bg-card px-3.5 text-body">
		<a href="https://github.com/mivinci/Anvil" target="_blank" rel="noreferrer" onclick={(event) => void openExternal(event, 'https://github.com/mivinci/Anvil')} class="flex items-center justify-between py-2.5 transition-colors hover:text-primary focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.settings.flow.projectUrl}<ExternalLink size={14} class="text-muted-foreground" aria-hidden="true" /></a>
		<a href="https://github.com/mivinci/Anvil/issues/new/choose" target="_blank" rel="noreferrer" onclick={(event) => void openExternal(event, 'https://github.com/mivinci/Anvil/issues/new/choose')} class="flex items-center justify-between py-2.5 transition-colors hover:text-primary focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.settings.flow.reportIssue}<ExternalLink size={14} class="text-muted-foreground" aria-hidden="true" /></a>
	</div>
	{#if error}<div role="alert" class="text-body text-destructive">{error}<button type="button" disabled={loading} onclick={() => void load()} class="ml-2 underline underline-offset-2">{localeState.messages.settings.flow.retry}</button></div>{/if}
</div>
