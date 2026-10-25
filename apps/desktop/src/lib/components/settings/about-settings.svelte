<script lang="ts">
	import { onMount } from 'svelte';
	import { isTauri } from '@tauri-apps/api/core';
	import { ExternalLink, FolderOpen, RefreshCw } from '@lucide/svelte';
	import { shell, type AppInfo, type RuntimeStatus, type LoggingStatus, type UpdateStatus } from '../../shell';
	import { getRuntimeFileLoggingStatus } from '../../runtime';
	let logging = $state<LoggingStatus | null>(null);
	let runtimeFileLogging = $state<boolean | null>(null);
	let logsLoading = $state(false);
	let logsOpening = $state(false);
	let logsError = $state<string | null>(null);
	async function loadLogs() {
		if (!isTauri() || logsLoading) return;
		logsLoading = true;
		logsError = null;
		const [native, health] = await Promise.allSettled([shell.getLoggingStatus(), getRuntimeFileLoggingStatus()]);
		logging = native.status === 'fulfilled' ? native.value : null;
		runtimeFileLogging = health.status === 'fulfilled' ? health.value : null;
		if (native.status === 'rejected') logsError = '暂时无法读取日志状态，请刷新重试。';
		logsLoading = false;
	}
	async function openLogs() {
		if (logsOpening) return;
		logsOpening = true;
		logsError = null;
		try { await shell.openLogDirectory(); }
		catch { logsError = '无法打开日志目录，请检查目录是否存在或稍后重试。'; }
		finally { logsOpening = false; }
	}
	function logState(available: boolean | null | undefined) {
		if (!isTauri()) return '仅桌面端可用';
		if (logsLoading) return '读取中…';
		return available === true ? '文件记录已启用' : available === false ? '文件记录不可用' : '状态不可用';
	}
	let info = $state<AppInfo | null>(null);
	let runtime = $state<RuntimeStatus | null>(null);
	let updates = $state<UpdateStatus | null>(null);
	let checkingUpdates = $state(false);
	let updateError = $state<string | null>(null);
	let updateTimer: ReturnType<typeof setTimeout> | undefined;
	let error = $state<string | null>(null);
	let loading = $state(true);
	async function load() {
		loading = true;
		error = null;
		if (!isTauri()) { loading = false; return; }
		try { [info, runtime, updates] = await Promise.all([shell.getAppInfo(), shell.getRuntimeStatus(), shell.getUpdateStatus()]); }
		catch { error = '暂时无法读取版本信息。'; }
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
			updateError = '无法读取更新状态，请稍后重试。';
			checkingUpdates = false;
		}
	}
	async function checkUpdates() {
		if (!isTauri() || checkingUpdates) return;
		checkingUpdates = true;
		updateError = null;
		try {
			await shell.checkForUpdates();
			await refreshUpdates();
		} catch {
			updateError = '无法检查更新，请稍后重试。';
			checkingUpdates = false;
		}
	}
	async function openShellRelease() {
		const url = updates?.shell.releaseUrl;
		if (!url) return;
		try { await shell.openExternalUrl(url); }
		catch { updateError = '无法打开 Shell 发布页，请稍后重试。'; }
	}
	onMount(() => {
		void load();
		void loadLogs();
		return () => { if (updateTimer) clearTimeout(updateTimer); };
	});
	async function openProject(event: MouseEvent) {
		if (!isTauri()) return;
		event.preventDefault();
		try { await shell.openExternalUrl('https://github.com/mivinci/Anvil'); }
		catch { error = '无法打开系统浏览器，请稍后重试。'; }
	}
</script>

<div class="space-y-5">
	<div class="flex items-center gap-3">
		<img src="/logo.png" alt="" width="40" height="40" class="size-10 shrink-0 rounded-xl object-cover" />
		<div class="min-w-0">
			<p class="text-xl font-semibold tracking-tight">Anvil</p>
			<p class="mt-1 text-[11px] leading-5 text-muted-foreground">Minecraft Java 版启动与服务端管理工具</p>
		</div>
	</div>
	<section aria-label="版本信息" class="space-y-2">
		<div class="flex items-center justify-between gap-3">
			<h3 class="text-[13px] font-medium">版本信息</h3>
			<button type="button" aria-label="检查更新" title="检查更新" disabled={!isTauri() || checkingUpdates} onclick={() => void checkUpdates()} class="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-[12px] text-muted-foreground transition-colors hover:bg-chrome-hover hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={13} class={checkingUpdates ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" />{checkingUpdates ? '检查中' : '检查更新'}</button>
		</div>
		<div class="divide-y divide-border rounded-lg bg-card px-3.5 text-[13px]">
			{#each [['Shell 版本', info?.shellVersion], ['Desktop App 版本', info?.appVersion], ['Runtime 版本', runtime?.runtimeVersion], ['Runtime API 协议', runtime?.apiVersion], ['目标平台', info?.targetPlatform]] as [label, value]}
				<div class="flex min-h-10 items-center justify-between gap-4 py-2"><span>{label}</span><span class="select-text text-right text-muted-foreground">{loading ? '读取中…' : value ?? (isTauri() ? '不可用' : '仅桌面端可用')}</span></div>
			{/each}
		</div>
		{#if updates?.shell.state === 'available'}
			<div class="flex items-center justify-between gap-3 rounded-lg border border-primary/20 bg-primary/5 px-3.5 py-2.5 text-[12px]">
				<span>Shell {updates.shell.availableVersion} 可用，需要重新安装。</span>
				<button type="button" onclick={() => void openShellRelease()} class="flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-primary hover:bg-primary/10 focus-visible:outline-2 focus-visible:outline-ring">打开发布页<ExternalLink size={12} aria-hidden="true" /></button>
			</div>
		{/if}
		{#if updateError}<p role="alert" class="text-[12px] text-destructive">{updateError}</p>{/if}
	</section>
	<section aria-label="日志" class="space-y-2">
		<div class="flex items-center justify-between gap-3">
			<h3 class="text-[13px] font-medium">日志</h3>
			<button type="button" aria-label="刷新日志状态" title="刷新日志状态" disabled={!isTauri() || logsLoading} onclick={() => void loadLogs()} class="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={13} aria-hidden="true" /></button>
		</div>
		<div class="divide-y divide-border rounded-lg bg-card px-3.5 text-[13px]">
			<div class="flex items-center justify-between gap-3 py-2">
				<div class="min-w-0"><p>日志目录</p><p class="mt-1 select-text break-all text-[11px] leading-4 text-muted-foreground">{logging?.directory ?? (logsLoading ? '读取中…' : isTauri() ? '目录不可用' : '仅桌面端可用')}</p></div>
				<button type="button" disabled={!logging?.directory || logsOpening} onclick={() => void openLogs()} class="flex shrink-0 items-center gap-1.5 rounded-md px-2 py-1.5 text-[12px] text-muted-foreground transition-colors hover:bg-chrome-hover hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><FolderOpen size={14} aria-hidden="true" />{logsOpening ? '打开中…' : '打开目录'}</button>
			</div>
			{#each [['Shell', logging?.fileLoggingAvailable], ['Runtime', runtimeFileLogging]] as [label, available]}
				<div class="flex min-h-9 items-center justify-between gap-3 py-2"><span>{label}</span><span class="text-right text-[12px] text-muted-foreground" class:text-destructive={available === false}>{logState(available as boolean | null | undefined)}</span></div>
			{/each}
		</div>
		<p class="text-[11px] leading-4 text-muted-foreground">shell.log / runtime.log · 每份 10 MiB，各保留 4 份历史文件</p>
		{#if logsError}<p role="alert" class="text-[12px] text-destructive">{logsError}</p>{/if}
	</section>
	<a href="https://github.com/mivinci/Anvil" target="_blank" rel="noreferrer" onclick={openProject} class="flex items-center justify-between rounded-lg bg-card px-3.5 py-3 text-[13px] transition-colors hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">项目地址<ExternalLink size={14} class="text-muted-foreground" aria-hidden="true" /></a>
	{#if error}<div role="alert" class="text-[13px] text-destructive">{error}<button type="button" disabled={loading} onclick={() => void load()} class="ml-2 underline underline-offset-2">重试</button></div>{/if}
</div>
