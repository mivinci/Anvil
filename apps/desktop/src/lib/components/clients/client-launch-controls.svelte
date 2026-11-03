<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import { Play, LoaderCircle, X } from '@lucide/svelte';
	import type { InstalledClient } from '../../runtime';
	import { getClientLaunchState, launchActive, launchLabel, launchNeedsLogin } from '../../client-launch-state.svelte.js';
	import { getAccountState } from '../../account-state.svelte.js';
	import { getClientInstallState } from '../../client-install-state.svelte.js';
	import { getModpackExportState } from '../../modpack-export-state.svelte.js';
	let { client, compact = false, large = false, feedback = !compact, reserveCancel = true, trailing }: {
		client: InstalledClient; compact?: boolean; large?: boolean; feedback?: boolean; reserveCancel?: boolean; trailing?: Snippet;
	} = $props();
	const launches = getClientLaunchState();
	const accounts = getAccountState();
	const installs = getClientInstallState();
	const exports = getModpackExportState();
	let hint = $state(false);
	let checking = $state(false);
	let authenticationAttemptAfter = $state<number | null>(null);
	const operation = $derived(launches.forClient(client.name));
	const exporting = $derived((exports.active && exports.operation?.name === client.name.toLowerCase()) || (exports.starting && exports.intent?.name === client.name.toLowerCase()));
	const active = $derived(launchActive(operation));
	const pending = $derived(checking || launches.pending.includes(client.name));
	const sessionExpired = $derived(!!accounts.currentAccount && !accounts.offlineMode && accounts.isExpired(accounts.currentAccount.id));
	const sessionVerifying = $derived(!!accounts.currentAccount && accounts.verifying === accounts.currentAccount.id);
	const idleLabel = $derived(sessionExpired ? localeState.messages.clients.flow.signInAgain : sessionVerifying ? localeState.messages.clients.flow.checkingSignIn : large ? localeState.messages.clients.flow.play : localeState.messages.clients.flow.launch);
	const label = $derived(pending ? localeState.messages.clients.flow.preparingShort : active && operation ? launchLabel(operation) : idleLabel);
	const preparing = $derived(pending || (active && operation?.phase !== 'running'));
	const buttonLabel = $derived.by(() => {
		if (pending) return localeState.messages.clients.flow.preparingShort;
		if (!active) return idleLabel;
		if (operation?.phase === 'running') return localeState.messages.clients.flow.running;
		return localeState.messages.clients.flow.preparingShort;
	});
	const canCancel = $derived(active && operation?.phase !== 'running');
	const downloading = $derived(active && operation?.javaStage === 'downloading');
	const progress = $derived(downloading && operation?.javaTotalBytes ? Math.min(100, operation.javaDownloadedBytes / operation.javaTotalBytes * 100) : null);
	// React only to a launch initiated here, not historical failures restored on mount.
	$effect(() => {
		if (operation?.errorCode === 'authentication_failed' || operation?.errorCode === 'account_required') {
			const id = accounts.currentAccount?.id;
			if (id) accounts.markExpired(id);
		}
	});
	$effect(() => {
		if (authenticationAttemptAfter === null || !operation || operation.id <= authenticationAttemptAfter) return;
		if (launchNeedsLogin(operation)) {
			authenticationAttemptAfter = null;
			accounts.openLoginDialog(() => { void launch(false, true); });
		} else if (!active || operation.phase === 'running') authenticationAttemptAfter = null;
	});
	async function launch(installJava = false, offlineTest = false) {
		if (exporting || sessionVerifying) return;
		if (sessionExpired) { accounts.openLoginDialog(); return; }
		checking = true; hint = false;
		try {
			if (offlineTest || accounts.offlineMode) { await launches.start(client.name, null, installJava, true); return; }
			if (!accounts.loaded && !await accounts.refreshAccounts()) { hint = true; return; }
			const account = accounts.currentAccount;
			if (!account) { accounts.openLoginDialog(() => { void launch(installJava, true); }); return; }
			authenticationAttemptAfter = operation?.id ?? 0;
			await launches.start(client.name, account.id, installJava);
		} finally { checking = false; }
	}
</script>
<div class={compact ? 'contents' : 'relative z-20 min-w-0'}>
<div class="relative z-20 flex shrink-0 flex-wrap items-center gap-1">
				<button type="button" title={exporting ? localeState.messages.clients.flow.exportingPack : sessionExpired ? localeState.messages.clients.flow.expiredNotice : sessionVerifying ? localeState.messages.clients.flow.checkingSession : label} aria-label={localeState.messages.clients.launchAction(client.name, exporting ? localeState.messages.clients.flow.exportingPack : sessionExpired ? localeState.messages.clients.flow.expiredNotice : sessionVerifying ? localeState.messages.clients.flow.checkingSession : label, preparing && progress !== null ? Math.floor(progress) : null)} aria-busy={preparing} disabled={active || pending || launches.uncertain || installs.active || exporting || sessionVerifying} onclick={() => void launch()} class={`relative isolate inline-flex shrink-0 items-center justify-center gap-2 overflow-hidden rounded-lg bg-primary text-primary-foreground transition-opacity enabled:hover:opacity-90 focus-visible:outline-2 focus-visible:outline-ring ${preparing ? '' : 'disabled:opacity-50'} ${compact ? 'size-7' : large ? `h-9 max-w-full px-3.5 text-xs font-medium ${preparing ? 'w-44' : 'w-30'}` : `h-8 max-w-full px-3 text-xs font-medium ${preparing ? 'w-40' : 'w-28'}`}`}>
					{#if preparing}<span aria-hidden="true" class={`pointer-events-none absolute inset-y-0 left-0 -z-10 bg-primary-foreground/15 ${progress === null ? 'launch-progress-indeterminate w-1/3' : 'transition-[width] duration-300 motion-reduce:transition-none'}`} style:width={progress !== null ? `${progress}%` : undefined}></span>{/if}
					{#if preparing}<LoaderCircle size={large ? 14 : 13} class="shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else}<Play size={large ? 14 : 13} fill="currentColor" class="shrink-0" aria-hidden="true" />{/if}{#if !compact}<span class="min-w-0 truncate">{buttonLabel}</span>{#if preparing && progress !== null}<span class="shrink-0 text-xs tabular-nums">{Math.floor(progress)}%</span>{/if}{/if}
				</button>
				{#if trailing}{@render trailing()}{/if}
				{#if canCancel || reserveCancel}<button type="button" aria-label={localeState.messages.clients.flow.cancelLaunch} title={localeState.messages.clients.flow.cancelLaunch} aria-hidden={!canCancel} tabindex={canCancel ? 0 : -1} disabled={!canCancel || operation?.cancelRequested} onclick={() => operation && void launches.cancel(operation)} class={`grid shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover disabled:opacity-50 ${large ? 'size-7' : 'size-6'} ${canCancel ? '' : 'invisible pointer-events-none'}`}><X size={large ? 13 : 12} aria-hidden="true" /></button>{/if}
			</div>
	{#if feedback}
	<div class={!compact && (launches.uncertain || hint || operation?.error) ? 'mt-2' : 'contents'}>
	{#if preparing && !compact}<span role="status" class="sr-only">{label}</span><progress aria-label={localeState.messages.clients.flow.launchProgress} max="100" value={progress ?? undefined} class="sr-only"></progress>{/if}
	{#if launches.uncertain}
		<div class="relative z-20 col-span-2 mt-2 flex items-center gap-2 px-1 text-xs text-muted-foreground"><span>{launches.error ?? localeState.messages.clients.flow.checkingRunning}</span><button type="button" class="text-primary" onclick={() => void launches.sync()}>{localeState.messages.clients.flow.refreshState}</button></div>
	{:else if !preparing && (hint || operation?.error)}
		<div class="relative z-20 col-span-2 mt-2 flex flex-wrap items-center gap-2 px-1 text-xs text-muted-foreground" role="status">
			<span>{hint ? localeState.messages.clients.flow.accountsFailed : launchLabel(operation!)}</span>
			{#if operation?.error}<details class="max-w-full"><summary>{localeState.messages.errors.technicalDetails}</summary><p class="mt-1 whitespace-pre-wrap break-words">{operation.error}</p></details>{/if}
			{#if operation?.errorCode === 'game_exit'}<a class="text-primary" href={`${resolve('/clients/[name]/logs', { name: client.name.toLowerCase() })}?source=console`}>{localeState.messages.clients.flow.viewGameLogs}</a>{/if}
			{#if operation?.errorCode === 'java_required'}<button type="button" disabled={pending || installs.active} class="text-primary disabled:opacity-50" onclick={() => void launch(true, operation?.offlineTest ?? false)}>{localeState.messages.clients.installJavaLaunch(operation.javaMajor, operation.offlineTest)}</button>
			{:else if launchNeedsLogin(operation)}<button type="button" class="text-primary" onclick={() => accounts.openLoginDialog()}>{localeState.messages.clients.flow.signInAgain}</button>
			{:else if operation?.errorCode === 'installation_conflict'}<button type="button" disabled={!installs.canStart} class="text-primary disabled:opacity-50" onclick={() => void installs.start(client.name, client.versionId, client.loader)}>{localeState.messages.clients.flow.repair}</button>{/if}
		</div>
	{/if}
	</div>
	{/if}
</div>

<style>
	.launch-progress-indeterminate {
		animation: launch-progress 1.6s ease-in-out infinite;
	}
	@keyframes launch-progress {
		from { transform: translateX(-100%); }
		to { transform: translateX(300%); }
	}
	@media (prefers-reduced-motion: reduce) {
		.launch-progress-indeterminate { animation: none; transform: translateX(100%); }
	}
</style>
