<script lang="ts">
	import { localeState } from '#lib/locale.svelte.js';
	import { DropdownMenu } from 'bits-ui';
	import { tick } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import {
		Check, ChevronRight, CircleHelp, ExternalLink, LogIn, LogOut, TriangleAlert,
		Monitor, Moon, Palette, Settings2, Sun, UserRound, UsersRound,
	} from '@lucide/svelte';
	import type { AppearanceMode } from './appearance-menu.svelte';
	import { getAccountState } from '../account-state.svelte.js';
	import type { AccountSummary } from '../runtime';
	import { shell } from '../shell';
	import AccountAvatar from './account-avatar.svelte';

	let { appearance, onAppearanceChange }: {
		appearance: AppearanceMode;
		onAppearanceChange: (mode: AppearanceMode) => void;
	} = $props();

	const appearanceOptions = $derived([
		{ value: 'system', label: localeState.messages.account.flow.system, icon: Monitor },
		{ value: 'light', label: localeState.messages.account.flow.light, icon: Sun },
		{ value: 'dark', label: localeState.messages.account.flow.dark, icon: Moon },
	] as const);
	const itemClass = 'flex min-h-7.5 cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-[12.5px] leading-4 outline-none transition-colors data-[highlighted]:bg-chrome-hover data-[disabled]:opacity-40';
	const subContentClass = 'z-40 min-w-40 rounded-xl border border-chrome-control-border bg-popover/95 p-1 text-popover-foreground shadow-[0_4px_16px_rgb(0_0_0/0.08)] backdrop-blur-xl outline-none';
	let open = $state(false);
	const accountState = getAccountState();
	let menuErrorValue = $state<string | (() => string | null) | null>(null);
	const menuError = $derived(typeof menuErrorValue === 'function' ? menuErrorValue() : menuErrorValue);
	const accounts = $derived(accountState.accounts);
	const loading = $derived(accountState.loading);
	const busy = $derived(accountState.busy);
	const loaded = $derived(accountState.loaded);
	const error = $derived(menuError ?? accountState.error?.message ?? null);
	const currentAccount = $derived(accountState.currentAccount);
	const currentExpired = $derived(!!currentAccount && accountState.isExpired(currentAccount.id));

	function refreshAccounts() {
		menuErrorValue = null;
		void accountState.retryAccountAction();
	}

	async function changeAccount(account: AccountSummary) {
		menuErrorValue = null;
		if (await accountState.switchAccount(account.id)) open = false;
	}

	async function signOut() {
		if (!currentAccount) return;
		menuErrorValue = null;
		if (await accountState.logout(currentAccount.id)) open = false;
	}

	async function openLogin() {
		open = false;
		await tick();
		accountState.openLoginDialog();
	}

	async function openWebsite(url: string) {
		try {
			await shell.openExternalUrl(url);
			open = false;
		} catch {
			menuErrorValue = () => (localeState.messages.account.flow.browserFailed);
		}
	}
</script>

<DropdownMenu.Root bind:open onOpenChange={(value) => {
	if (!value) return;
	menuErrorValue = null;
	if (!accountState.error) void accountState.refreshAccounts();
}}>
	<DropdownMenu.Trigger
		data-account-trigger
		class="relative grid size-9 cursor-default place-items-center [clip-path:url(#nav-button-shape)] text-chrome-muted/80 transition-colors hover:bg-chrome-selected hover:text-chrome-muted focus-visible:outline-2 focus-visible:outline-ring data-[state=open]:bg-chrome-selected"
		aria-label={currentAccount ? localeState.messages.account.accountLabel(currentAccount.username, currentExpired) : localeState.messages.account.flow.account}
		title={localeState.messages.account.flow.account}
	>
		{#if currentAccount}
			<AccountAvatar account={currentAccount} size={24} />
		{:else}
			<UserRound size={19} aria-hidden="true" />
		{/if}
	</DropdownMenu.Trigger>
	<DropdownMenu.Content
		side="right"
		align="end"
		sideOffset={8}
		collisionPadding={8}
		class="z-30 w-52 rounded-xl border border-chrome-control-border bg-popover/95 p-1 text-popover-foreground shadow-[0_4px_16px_rgb(0_0_0/0.08)] backdrop-blur-xl outline-none"
		aria-label={localeState.messages.account.flow.menu}
	>
		<div class="flex min-w-0 items-center gap-2.5 px-2 py-2">
			<div class="flex size-7 shrink-0 items-center justify-center rounded-full bg-chrome-selected text-chrome-muted">
				{#if currentAccount}
					<AccountAvatar account={currentAccount} />
				{:else}
					<UserRound size={17} strokeWidth={1.75} aria-hidden="true" />
				{/if}
			</div>
			<div class="min-w-0">
				<p class="truncate text-body leading-4 font-medium">{currentAccount?.username ?? localeState.messages.account.flow.localPlayer}</p>
				<p class="mt-0.5 text-xs leading-4 text-chrome-muted">{accountState.loginRunning || accountState.loginStarting ? localeState.messages.account.flow.loggingIn : loading ? localeState.messages.account.flow.loadingAccounts : accountState.verifying ? localeState.messages.account.flow.verifyingLogin : currentAccount ? (currentExpired ? localeState.messages.account.flow.loginExpiredHint : localeState.messages.account.flow.minecraftJava) : loaded ? localeState.messages.account.flow.notSignedIn : localeState.messages.account.flow.microsoft}</p>
			</div>
		</div>
		<DropdownMenu.Separator class="mx-1.5 my-1 h-px bg-border/60" />
		{#if error}
			<div class="flex items-center justify-between gap-2 px-2 py-1.5 text-xs leading-4 text-chrome-muted" role="alert">
				<span class="truncate">{loaded ? localeState.messages.account.flow.actionIncomplete : localeState.messages.account.flow.unavailable}</span>
				<button
					type="button"
					class="shrink-0 rounded px-1 text-foreground/70 hover:bg-chrome-hover hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"
					disabled={loading || busy}
					onclick={() => void refreshAccounts()}
				>{localeState.messages.account.flow.retry}</button>
			</div>
			<details class="px-2 pb-2 text-xs text-chrome-muted"><summary>{localeState.messages.account.flow.errorDetails}</summary><p class="mt-1 break-words">{menuError ?? accountState.error?.detail}</p></details>
		{/if}
		<DropdownMenu.Item class={itemClass} disabled={loading || accountState.pendingAction !== null} onSelect={() => void openLogin()}>
			<LogIn size={14} strokeWidth={1.75} aria-hidden="true" />{accountState.loginRunning || accountState.loginStarting ? localeState.messages.account.flow.viewLogin : currentAccount ? (currentExpired ? localeState.messages.account.flow.reauthenticate : localeState.messages.account.flow.addAccount) : localeState.messages.account.flow.microsoftSignIn}
		</DropdownMenu.Item>
		<DropdownMenu.Sub>
			<DropdownMenu.SubTrigger class={itemClass} disabled={accounts.length === 0 || busy || loading}>
				<UsersRound size={14} strokeWidth={1.75} aria-hidden="true" /><span class="flex-1">{localeState.messages.account.flow.switchAccount}</span><ChevronRight size={12} class="text-chrome-muted" aria-hidden="true" />
			</DropdownMenu.SubTrigger>
			<DropdownMenu.SubContent sideOffset={4} collisionPadding={8} class={subContentClass}>
				{#each accounts as account (account.id)}
					<DropdownMenu.Item class={itemClass} onSelect={(event) => { event.preventDefault(); void changeAccount(account); }}>
						<AccountAvatar {account} size={20} /><span class="max-w-40 flex-1 truncate">{account.username}</span>
						{#if accountState.isExpired(account.id)}<TriangleAlert size={13} class="shrink-0 text-amber-600 dark:text-amber-400" aria-label={localeState.messages.account.flow.expired} />{:else if account.selected}<Check size={14} aria-label={localeState.messages.account.flow.current} />{/if}
					</DropdownMenu.Item>
				{/each}
			</DropdownMenu.SubContent>
		</DropdownMenu.Sub>
		<DropdownMenu.Separator class="mx-1.5 my-1 h-px bg-border/60" />
		<DropdownMenu.Sub>
			<DropdownMenu.SubTrigger class={itemClass}>
				<Palette size={14} strokeWidth={1.75} aria-hidden="true" /><span class="flex-1">{localeState.messages.account.flow.appearance}</span><ChevronRight size={12} class="text-chrome-muted" aria-hidden="true" />
			</DropdownMenu.SubTrigger>
			<DropdownMenu.SubContent sideOffset={4} collisionPadding={8} class={subContentClass}>
				<DropdownMenu.Group>
					<DropdownMenu.GroupHeading class="px-2 py-1 text-xs font-medium text-chrome-muted">{localeState.messages.account.flow.appearance}</DropdownMenu.GroupHeading>
					{#each appearanceOptions as option (option.value)}
						{@const Icon = option.icon}
						<DropdownMenu.Item class={itemClass} onSelect={() => onAppearanceChange(option.value)}>
							<Icon size={14} strokeWidth={1.75} aria-hidden="true" /><span class="flex-1">{option.label}</span>
							{#if appearance === option.value}<Check size={14} aria-label={localeState.messages.account.flow.currentAppearance} />{/if}
						</DropdownMenu.Item>
					{/each}
				</DropdownMenu.Group>
			</DropdownMenu.SubContent>
		</DropdownMenu.Sub>
		<DropdownMenu.Item class={itemClass} onSelect={() => goto(resolve('/settings/general'))}>
			<Settings2 size={14} strokeWidth={1.75} aria-hidden="true" />{localeState.messages.account.flow.settings}
		</DropdownMenu.Item>
		<DropdownMenu.Sub>
			<DropdownMenu.SubTrigger class={itemClass}>
				<CircleHelp size={14} strokeWidth={1.75} aria-hidden="true" /><span class="flex-1">{localeState.messages.account.flow.help}</span><ChevronRight size={12} class="text-chrome-muted" aria-hidden="true" />
			</DropdownMenu.SubTrigger>
			<DropdownMenu.SubContent sideOffset={4} collisionPadding={8} class={subContentClass}>
				<DropdownMenu.Item class={itemClass} onSelect={(event) => { event.preventDefault(); void openWebsite('https://www.minecraft.net/'); }}>
					<span class="flex-1">{localeState.messages.account.flow.minecraftWebsite}</span><ExternalLink size={12} class="text-chrome-muted" aria-hidden="true" />
				</DropdownMenu.Item>
				<DropdownMenu.Item class={itemClass} onSelect={(event) => { event.preventDefault(); void openWebsite('https://help.minecraft.net/'); }}>
					<span class="flex-1">{localeState.messages.account.flow.minecraftHelp}</span><ExternalLink size={12} class="text-chrome-muted" aria-hidden="true" />
				</DropdownMenu.Item>
			</DropdownMenu.SubContent>
		</DropdownMenu.Sub>
		<DropdownMenu.Separator class="mx-1.5 my-1 h-px bg-border/60" />
		<DropdownMenu.Item class={itemClass} disabled={!currentAccount || busy || loading} onSelect={(event) => { event.preventDefault(); void signOut(); }}>
			<LogOut size={14} strokeWidth={1.75} aria-hidden="true" />{localeState.messages.account.flow.signOut}
		</DropdownMenu.Item>
	</DropdownMenu.Content>
</DropdownMenu.Root>
