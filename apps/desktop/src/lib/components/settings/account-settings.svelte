<script lang="ts">
	import { localeState } from '#lib/locale.svelte.js';
	import { onMount } from 'svelte';
	import { Dialog } from 'bits-ui';
	import { LoaderCircle, LogIn, RefreshCw, Trash2, UserRound } from '@lucide/svelte';
	import AccountAvatar from '../account-avatar.svelte';
	import { getAccountState } from '../../account-state.svelte.js';
	import { getClientLaunchState, launchActive } from '../../client-launch-state.svelte.js';
	import { PlayStatsState } from '../../play-stats-state.svelte.js';
	import { formatPlayCount, formatPlayDuration } from '../../play-stats';
	import type { PlayStatsClearTarget } from '../../runtime';

	const accounts = getAccountState();
	const launches = getClientLaunchState();
	const stats = new PlayStatsState();
	const current = $derived(accounts.currentAccount);
	const history = $derived(current ? stats.snapshot?.accounts[current.id] : null);
	const playing = $derived(launches.pending.length > 0 || launches.operations.some(launchActive));
	const blocked = $derived(playing || launches.uncertain || stats.loading || stats.clearing || !stats.snapshot || stats.available === false);
	let confirmation = $state<{ target: PlayStatsClearTarget; username: string | null } | null>(null);
	let messageValue = $state<string | (() => string | null) | null>(null);
	const message = $derived(typeof messageValue === 'function' ? messageValue() : messageValue);
	const rowButton = 'inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg px-2.5 text-xs transition-colors focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	const secondary = 'h-8 rounded-lg px-3 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';

	onMount(() => { void stats.refresh(); return () => stats.dispose(); });

	function openConfirmation(target: PlayStatsClearTarget) {
		if (blocked || stats.clearUncertain) return;
		messageValue = null;
		stats.clearError = null;
		confirmation = { target, username: target.kind === 'account' ? current?.username ?? null : null };
	}

	function finish(target: PlayStatsClearTarget, reconciled = false) {
		const clearedCache = target.kind !== 'all' || launches.clearRecentHistory();
		const username = confirmation?.username ?? null;
		messageValue = () => (!clearedCache ? localeState.messages.account.flow.cacheClearFailed
			: reconciled ? localeState.messages.account.flow.confirmedEmpty
			: target.kind === 'all' ? localeState.messages.account.flow.allCleared : localeState.messages.account.clearedAccount(username));
		confirmation = null;
	}

	async function confirm() {
		if (!confirmation || blocked || stats.clearUncertain) return;
		const { target } = confirmation;
		if (await stats.clear(target)) finish(target);
	}

	async function reconcile() {
		const target = confirmation?.target;
		if (!target || !await stats.refresh()) return;
		const empty = target.kind === 'all' ? Object.keys(stats.snapshot?.accounts ?? {}).length === 0 : !stats.snapshot?.accounts[target.accountId];
		if (empty) finish(target, true);
		else { confirmation = null; messageValue = () => (localeState.messages.account.flow.rereadNotice); }
	}
</script>

<section aria-label={localeState.messages.account.flow.current} class="flex items-center justify-between gap-3 rounded-lg bg-card p-3.5">
	<div class="flex min-w-0 items-center gap-3">
		{#if current}<AccountAvatar account={current} size={36} />{:else}<span class="grid size-9 shrink-0 place-items-center rounded-md bg-chrome-hover text-muted-foreground"><UserRound size={19} aria-hidden="true" /></span>{/if}
		<div class="min-w-0"><p class="truncate text-[13px] font-medium">{current?.username ?? localeState.messages.account.flow.noAccount}</p><p class="mt-0.5 text-[11px] text-muted-foreground">{current ? localeState.messages.account.flow.microsoft : localeState.messages.account.flow.historyLoginHint}</p></div>
	</div>
	<button type="button" class={`${rowButton} text-muted-foreground hover:bg-chrome-hover hover:text-foreground`} disabled={accounts.busy} onclick={() => accounts.openLoginDialog()}><LogIn size={13} aria-hidden="true" />{current ? localeState.messages.account.flow.addAccount : localeState.messages.account.flow.signIn}</button>
</section>

<section aria-labelledby="play-history-title" class="mt-6">
	<div class="mb-2 flex items-center justify-between gap-2"><h3 id="play-history-title" class="text-[13px] font-medium">{localeState.messages.account.flow.playHistory}</h3><button type="button" class="grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40" disabled={stats.loading || stats.clearing} onclick={() => void stats.refresh()} aria-label={localeState.messages.account.flow.refreshHistory}><RefreshCw size={13} class={stats.loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button></div>
	<p class="mb-3 text-[11px] leading-5 text-muted-foreground">{localeState.messages.account.flow.localHistoryHint}</p>
	<div class="divide-y divide-border/60 rounded-lg bg-card px-3.5">
		<div class="flex items-center justify-between gap-4 py-3">
			<div class="min-w-0"><p class="text-[13px] font-medium">{localeState.messages.account.flow.accountHistory}</p><p class="mt-1 text-[11px] leading-5 text-muted-foreground">{!current ? localeState.messages.account.flow.selectAccountHint : stats.loading ? localeState.messages.account.flow.readingHistory : stats.error || stats.available === false ? localeState.messages.account.flow.historyUnavailable : history ? localeState.messages.account.historySummary(current.username, formatPlayCount(history.launches, localeState.current), formatPlayDuration(history.totalMs, localeState.current)) : localeState.messages.account.noHistory(current.username)}</p></div>
			<button type="button" class={`${rowButton} text-destructive enabled:hover:bg-destructive/10`} disabled={blocked || !current || !history || stats.clearUncertain} onclick={() => current && openConfirmation({ kind: 'account', accountId: current.id })}><Trash2 size={13} aria-hidden="true" />{localeState.messages.account.flow.clearHistory}</button>
		</div>
		<div class="flex items-center justify-between gap-4 py-3">
			<div class="min-w-0"><p class="text-[13px] font-medium">{localeState.messages.account.flow.allHistory}</p><p class="mt-1 text-[11px] leading-5 text-muted-foreground">{localeState.messages.account.flow.allHistoryHint}</p></div>
			<button type="button" class={`${rowButton} text-muted-foreground enabled:hover:bg-destructive/10 enabled:hover:text-destructive`} disabled={blocked || stats.clearUncertain || (!Object.keys(stats.snapshot?.accounts ?? {}).length && !launches.recentPlays.length)} onclick={() => openConfirmation({ kind: 'all' })}>{localeState.messages.account.flow.clearAll}</button>
		</div>
	</div>
	{#if playing}<p class="mt-2 text-[11px] leading-5 text-muted-foreground">{localeState.messages.account.flow.stopBeforeClear}</p>
	{:else if launches.uncertain}<p class="mt-2 text-[11px] leading-5 text-muted-foreground">{localeState.messages.account.flow.runningUnknown}<button type="button" class="text-primary hover:underline" onclick={() => void launches.sync()}>{localeState.messages.account.flow.refreshStatus}</button></p>{/if}
	{#if stats.available === false}<p class="mt-2 text-[11px] text-muted-foreground">{localeState.messages.account.flow.historyUnsupported}</p>{/if}
	{#if stats.error}<p role="alert" class="mt-2 text-[11px] leading-5 text-destructive">{stats.error}</p>{/if}
	{#if stats.errorDetail}<details class="mt-2 text-[11px] text-muted-foreground"><summary>{localeState.messages.account.flow.errorDetails}</summary><p class="break-words">{stats.errorDetail}</p></details>{/if}
	{#if stats.clearUncertain && !confirmation}<p role="alert" class="mt-2 text-[11px] leading-5 text-muted-foreground">{stats.clearError} <button type="button" class="text-primary hover:underline" disabled={stats.loading} onclick={() => void stats.refresh()}>{localeState.messages.account.flow.rereadHistory}</button></p>{/if}
	{#if message}<p role="status" class="mt-2 text-[11px] leading-5 text-muted-foreground">{message}</p>{/if}
</section>

<Dialog.Root open={confirmation !== null} onOpenChange={open => { if (!open && !stats.clearing) confirmation = null; }}>
	<Dialog.Portal to=".app-window">
		<Dialog.Overlay class="fixed inset-0 z-40 bg-black/25" />
		<Dialog.Content class="fixed top-1/2 left-1/2 z-50 w-[min(360px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border bg-popover p-4 text-popover-foreground shadow-lg outline-none" onEscapeKeydown={event => { if (stats.clearing) event.preventDefault(); }} onInteractOutside={event => { if (stats.clearing) event.preventDefault(); }}>
			<Dialog.Title class="text-sm font-semibold">{confirmation?.target.kind === 'all' ? localeState.messages.account.flow.clearAllTitle : localeState.messages.account.confirmClearAccount(confirmation?.username ?? null)}</Dialog.Title>
			<Dialog.Description class="mt-2 text-xs leading-6 text-muted-foreground">{confirmation?.target.kind === 'all' ? localeState.messages.account.flow.clearAllDescription : localeState.messages.account.flow.clearAccountDescription} {localeState.messages.account.flow.clearIrreversible}</Dialog.Description>
			{#if stats.clearError}<p role="alert" class="mt-3 text-xs leading-5 text-destructive">{stats.clearError}</p>{/if}
			{#if stats.clearErrorDetail}<details class="mt-2 text-[11px] text-muted-foreground"><summary>{localeState.messages.account.flow.errorDetails}</summary><p class="break-words">{stats.clearErrorDetail}</p></details>{/if}
			<div class="mt-4 flex justify-end gap-1.5">
				<button type="button" class={secondary} disabled={stats.clearing} onclick={() => confirmation = null}>{localeState.messages.account.flow.cancel}</button>
				{#if stats.clearUncertain}<button type="button" class={secondary} disabled={stats.loading || stats.clearing} onclick={() => void reconcile()}>{stats.loading ? localeState.messages.account.flow.reading : localeState.messages.account.flow.rereadHistory}</button>
				{:else}<button type="button" class="inline-flex h-8 items-center gap-1.5 rounded-lg bg-destructive px-3 text-xs font-medium text-white focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40" disabled={blocked} onclick={() => void confirm()}>{#if stats.clearing}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.account.flow.clearing}{:else}{localeState.messages.account.flow.confirmClear}{/if}</button>{/if}
			</div>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
