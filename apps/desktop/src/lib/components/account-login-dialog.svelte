<script lang="ts">
	import { Dialog } from 'bits-ui';
	import { isTauri } from '@tauri-apps/api/core';
	import { Check, CircleAlert, CircleCheck, CircleSlash, Clock3, Copy, ExternalLink, FlaskConical, LoaderCircle, UserRound, X } from '@lucide/svelte';
	import { getAccountState } from '../account-state.svelte.js';
	import { loginPhase, remainingTime, verificationUrl } from '../account-login';
	import { shell } from '../shell';

	const accounts = getAccountState();
	const buttonClass = 'inline-flex h-8 cursor-default items-center justify-center gap-1.5 rounded-lg px-3 text-xs font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:opacity-40';
	const primaryClass = `${buttonClass} bg-primary text-primary-foreground enabled:hover:bg-primary/90`;
	const secondaryClass = `${buttonClass} text-chrome-muted enabled:hover:bg-chrome-hover enabled:hover:text-foreground`;
	let now = $state(Date.now());
	let copied = $state(false);
	let copying = $state(false);
	let browserOpening = $state(false);
	let utilityError = $state<string | null>(null);
	let openedOperationId = $state<number | null>(null);
	let restartAfterCancel = $state(false);

	const operation = $derived(accounts.login);
	const authorization = $derived(operation?.authorization ?? null);
	const expiresAt = $derived(authorization?.expiresAt ?? null);
	const phase = $derived(loginPhase(operation, accounts.loginStarting, accounts.loginCancelling, now));
	const url = $derived(verificationUrl(authorization?.verificationUri));
	const showingCode = $derived(phase === 'code' && authorization !== null);
	const error = $derived(accounts.loginError ?? (showingCode ? utilityError ?? (!url ? '验证页面地址不可用，请取消后重试。' : null) : null));
	const title = $derived(phase === 'success' ? '登录成功' : '登录 Microsoft 账户');
	const progress = $derived(phase === 'verifying' ? '正在验证游戏账户…' : phase === 'saving' ? '正在保存账户…' : phase === 'cancelling' ? '正在取消登录…' : '正在获取验证码…');

	$effect(() => {
		// This timer is display-only; Microsoft polling remains owned by Core.
		if (!accounts.loginDialogOpen || expiresAt === null) return;
		now = Date.now();
		if (Date.now() >= expiresAt * 1_000) return;
		const timer = setInterval(() => {
			now = Date.now();
			if (Date.now() >= expiresAt * 1_000) clearInterval(timer);
		}, 1_000);
		return () => clearInterval(timer);
	});

	$effect(() => {
		if (!accounts.loginDialogOpen) {
			restartAfterCancel = false;
			copied = false;
			utilityError = null;
			return;
		}
		if (!restartAfterCancel || accounts.loginStarting || accounts.loginCancelling || accounts.loginRunning) return;
		restartAfterCancel = false;
		if (operation?.state === 'cancelled' || operation?.state === 'failed') void accounts.startLogin();
	});

	$effect(() => {
		if (!accounts.loginDialogOpen || !showingCode || !url || !operation || !isTauri()) return;
		if (openedOperationId === operation.id) return;
		openedOperationId = operation.id;
		void openVerificationPage();
	});

	async function copyCode() {
		if (!showingCode || !authorization || copying) return;
		now = Date.now();
		if (authorization.expiresAt * 1_000 <= Date.now()) return;
		const code = authorization.userCode;
		const id = operation?.id;
		copying = true;
		utilityError = null;
		try {
			await navigator.clipboard.writeText(code);
			if (accounts.loginDialogOpen && accounts.login?.id === id) copied = true;
		} catch {
			if (accounts.loginDialogOpen && accounts.login?.id === id) utilityError = '复制失败，可选中验证码手动复制。';
		} finally {
			copying = false;
		}
	}

	async function openVerificationPage() {
		if (!showingCode || !url || browserOpening) return;
		now = Date.now();
		if (!authorization || authorization.expiresAt * 1_000 <= Date.now()) return;
		const id = operation?.id;
		browserOpening = true;
		utilityError = null;
		try {
			if (isTauri()) await shell.openExternalUrl(url);
			else window.open(url, '_blank', 'noopener,noreferrer');
		} catch {
			if (accounts.loginDialogOpen && accounts.login?.id === id) utilityError = '无法打开浏览器，请重试下方按钮。';
		} finally {
			browserOpening = false;
		}
	}

	function retryLogin() {
		utilityError = null;
		copied = false;
		if (accounts.loginRunning) {
			restartAfterCancel = true;
			void accounts.cancelLogin();
		} else {
			void accounts.startLogin();
		}
	}

	function close() {
		accounts.closeLoginDialog();
	}
</script>

<Dialog.Root open={accounts.loginDialogOpen} onOpenChange={(open) => { if (!open) close(); }}>
	<Dialog.Overlay class="fixed inset-0 z-40 bg-black/15 backdrop-blur-[2px]" />
	<Dialog.Content
		class="fixed top-1/2 left-1/2 z-50 w-90 max-w-[calc(100vw-32px)] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-chrome-control-border bg-popover p-5 text-popover-foreground shadow-[0_12px_48px_rgb(0_0_0/0.16)] outline-none"
		onInteractOutside={(event) => event.preventDefault()}
		onEscapeKeydown={(event) => { if (phase === 'saving') event.preventDefault(); }}
		onCloseAutoFocus={(event) => {
			event.preventDefault();
			document.querySelector<HTMLButtonElement>('[data-account-trigger]')?.focus();
		}}
	>
		<button type="button" class="absolute top-3 right-3 grid size-6 place-items-center rounded-md text-chrome-muted transition-colors hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-30" aria-label="关闭登录窗口" disabled={phase === 'saving'} onclick={close}><X size={14} aria-hidden="true" /></button>
		<div class="mb-3 flex size-9 items-center justify-center rounded-xl bg-chrome-hover text-chrome-muted"><UserRound size={20} strokeWidth={1.75} aria-hidden="true" /></div>
		<Dialog.Title class="pr-5 text-sm font-semibold">{title}</Dialog.Title>
		<Dialog.Description class="mt-1 text-xs leading-5 text-chrome-muted">
			{phase === 'success' ? '账户已添加，并设为当前账户。' : '使用拥有 Minecraft Java 版的 Microsoft 账户。'}
		</Dialog.Description>

		<div class="mt-4">
			{#if phase === 'intro'}
				<p class="text-xs leading-5 text-chrome-muted">点击继续后，在系统浏览器中完成登录。Anvil 不会读取或保存你的 Microsoft 密码。</p>
			{:else if phase === 'code' && authorization}
				<div class="rounded-xl bg-chrome-hover px-3 py-3 text-center">
					<p class="mb-2 text-[11px] text-chrome-muted">在验证页面输入此验证码</p>
					<code class="block select-all font-mono text-2xl leading-8 font-semibold tracking-[0.15em] break-all" aria-label="验证码">{authorization.userCode}</code>
					<button type="button" class={`${secondaryClass} mt-1 h-7 px-2 text-[11px]`} disabled={copying} onclick={() => void copyCode()}>
						{#if copied}<Check size={12} aria-hidden="true" />已复制{:else}<Copy size={12} aria-hidden="true" />复制验证码{/if}
					</button>
				</div>
				<div class="mt-2 flex items-center justify-between gap-2 text-[11px] text-chrome-muted">
					<span role="status">等待浏览器授权…</span><span>有效期 {remainingTime(authorization.expiresAt, now)}</span>
				</div>
			{:else if phase === 'success'}
				<div class="flex items-center gap-3 rounded-xl bg-chrome-hover p-3">
					<CircleCheck size={24} class="shrink-0 text-primary" strokeWidth={1.75} aria-hidden="true" />
					<p class="truncate text-sm font-medium" role="status">{operation?.account?.username}</p>
				</div>
			{:else if phase === 'expired'}
				<div class="flex items-center gap-2.5 py-2 text-chrome-muted" role="status"><Clock3 size={20} class="shrink-0" aria-hidden="true" /><p class="text-xs leading-5">验证码已过期，请重新获取。</p></div>
			{:else if phase === 'cancelled'}
				<div class="flex items-center gap-2.5 py-2 text-chrome-muted" role="status"><CircleSlash size={20} class="shrink-0" aria-hidden="true" /><p class="text-xs leading-5">登录已取消。</p></div>
			{:else if phase === 'failed'}
				<div class="flex items-center gap-2.5 py-2 text-chrome-muted" role="status"><CircleAlert size={20} class="shrink-0" aria-hidden="true" /><p class="text-xs leading-5">未能完成本次登录。</p></div>
			{:else}
				<div class="flex items-center gap-2.5 py-3 text-chrome-muted" role="status"><LoaderCircle size={18} class="shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" /><p class="text-xs leading-5">{progress}</p></div>
			{/if}
			{#if error}<p class="mt-3 rounded-lg bg-destructive/8 px-2.5 py-2 text-xs leading-5 text-destructive" role="alert">{error}</p>{/if}
			{#if error && accounts.loginErrorDetail}
				<details class="mt-2 text-[11px] leading-4 text-chrome-muted">
					<summary class="cursor-default rounded py-1 focus-visible:outline-2 focus-visible:outline-ring">错误详情</summary>
					<p class="mt-1 max-h-28 overflow-y-auto rounded-lg bg-chrome-hover px-2.5 py-2 select-text whitespace-pre-wrap wrap-break-word">{accounts.loginErrorDetail}</p>
				</details>
			{/if}
		</div>

		<div class="mt-5 flex items-center justify-end gap-1.5">
			{#if import.meta.env.DEV && phase !== 'success'}
				<button type="button" class={`${secondaryClass} mr-auto`} disabled={accounts.loginStarting || accounts.loginRunning || accounts.loginCancelling} title="使用本地玩家 AnvilTester" onclick={() => accounts.useOfflineMode()}><FlaskConical size={13} aria-hidden="true" />离线模式</button>
			{/if}
			{#if phase === 'success'}
				<button type="button" class={primaryClass} onclick={close}>完成</button>
			{:else if accounts.loginError && accounts.loginRunning}
				<button type="button" class={secondaryClass} disabled={!accounts.canCancelLogin} onclick={() => void accounts.cancelLogin()}>取消登录</button>
				<button type="button" class={primaryClass} disabled={accounts.loginPolling || accounts.loginStarting || accounts.loginCancelling} onclick={() => void accounts.resumeLogin()}>重新查询</button>
			{:else if phase === 'code'}
				<button type="button" class={secondaryClass} disabled={!accounts.canCancelLogin} onclick={() => void accounts.cancelLogin()}>取消登录</button>
				<button type="button" class={primaryClass} disabled={!url || browserOpening} onclick={() => void openVerificationPage()}><ExternalLink size={13} aria-hidden="true" />打开验证页面</button>
			{:else if phase === 'expired'}
				<button type="button" class={secondaryClass} onclick={close}>关闭</button>
				<button type="button" class={primaryClass} onclick={retryLogin}>重新获取验证码</button>
			{:else if phase === 'intro'}
				<button type="button" class={secondaryClass} onclick={close}>取消</button>
				{#if accounts.configuration?.enabled}
					<button type="button" class={primaryClass} disabled={accounts.busy || accounts.configurationLoading} onclick={() => void accounts.startLogin()}>{accounts.loginError ? '重试登录' : '继续'}</button>
				{:else}
					<button type="button" class={primaryClass} disabled={accounts.configurationLoading} onclick={() => void accounts.loadLoginConfiguration()}>{accounts.configurationLoading ? '正在连接…' : '重新连接'}</button>
				{/if}
			{:else if phase === 'failed' || phase === 'cancelled'}
				<button type="button" class={secondaryClass} onclick={close}>关闭</button>
				<button type="button" class={primaryClass} disabled={accounts.busy} onclick={retryLogin}>重新登录</button>
			{:else}
				<button type="button" class={secondaryClass} disabled={!accounts.canCancelLogin} onclick={() => void accounts.cancelLogin()}>{phase === 'saving' ? '正在保存…' : phase === 'cancelling' ? '正在取消…' : '取消登录'}</button>
			{/if}
		</div>
	</Dialog.Content>
</Dialog.Root>
