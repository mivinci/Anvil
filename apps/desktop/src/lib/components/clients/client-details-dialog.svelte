<script lang="ts">
	import { Dialog } from 'bits-ui';
	import { Box, Check, Copy, Play, X } from '@lucide/svelte';
	import type { InstalledClient } from '../../runtime';
	let { client, onClose }: { client: InstalledClient; onClose: () => void } = $props();
	let copied = $state<string | null>(null);
	let copyError = $state<string | null>(null);
	let copying = $state(false);
	async function copy(label: string, value: string) {
		if (copying) return;
		copying = true;
		copied = null;
		copyError = null;
		try { await navigator.clipboard.writeText(value); copied = label; }
		catch { copyError = '无法复制，请选中文字后手动复制。'; }
		finally { copying = false; }
	}
</script>

<Dialog.Root open onOpenChange={(open) => { if (!open) onClose(); }}>
	<Dialog.Overlay class="fixed inset-0 z-40 bg-black/15 backdrop-blur-[2px]" />
	<Dialog.Content class="fixed top-1/2 left-1/2 z-50 max-h-[calc(100dvh-48px)] w-[400px] max-w-[calc(100vw-48px)] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-2xl border border-border bg-popover p-5 text-popover-foreground shadow-[0_12px_48px_rgb(0_0_0/0.16)] outline-none" onCloseAutoFocus={(event) => { event.preventDefault(); const trigger = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-client-details-trigger]')).find((button) => button.dataset.clientDetailsTrigger === client.id); (trigger ?? document.querySelector<HTMLButtonElement>('[data-client-install-trigger]'))?.focus(); }}>
		<button type="button" aria-label="关闭实例详情" onclick={onClose} class="absolute top-4 right-4 grid size-6 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring"><X size={14} aria-hidden="true" /></button>
		<div class="mb-3 grid size-10 place-items-center rounded-xl bg-primary/8 text-primary"><Box size={22} strokeWidth={1.5} aria-hidden="true" /></div>
		<Dialog.Title class="pr-5 text-base font-semibold">实例详情</Dialog.Title>
		<Dialog.Description class="mt-1 text-[11px] text-muted-foreground">查看已安装的 Minecraft 客户端信息。</Dialog.Description>
		<dl class="mt-4 divide-y divide-border rounded-lg border border-border px-3.5">
			{#each [{ label: '实例名称', value: client.name }, { label: '游戏版本', value: client.versionId }] as field}
				<div class="flex min-h-10 items-center justify-between gap-4 py-2.5"><dt class="shrink-0 text-[13px]">{field.label}</dt><dd class="flex min-w-0 items-center gap-1"><span class="min-w-0 select-text break-all text-right text-xs text-muted-foreground">{field.value}</span><button type="button" disabled={copying} aria-label={`复制${field.label}`} title={`复制${field.label}`} onclick={() => void copy(field.label, field.value)} class="grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover disabled:opacity-40 focus-visible:outline-2 focus-visible:outline-ring">{#if copied === field.label}<Check size={12} class="text-primary" aria-hidden="true" />{:else}<Copy size={12} aria-hidden="true" />{/if}</button></dd></div>
			{/each}
			<div class="flex min-h-10 items-center justify-between gap-4 py-2.5"><dt class="text-[13px]">游戏类型</dt><dd class="text-xs text-muted-foreground">Minecraft Java · 原版</dd></div>
			<div class="flex min-h-10 items-center justify-between gap-4 py-2.5"><dt class="text-[13px]">安装状态</dt><dd class="text-xs text-muted-foreground">已安装</dd></div>
		</dl>
		{#if copied}<p role="status" class="mt-2 text-[11px] text-muted-foreground">已复制{copied}</p>{/if}
		{#if copyError}<p role="alert" class="mt-2 text-[11px] text-destructive">{copyError}</p>{/if}
		<div class="mt-4 flex items-center justify-between gap-3"><p class="text-[11px] text-muted-foreground">客户端启动功能尚未开放。</p><button type="button" disabled class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground disabled:opacity-40"><Play size={12} aria-hidden="true" />启动游戏</button></div>
	</Dialog.Content>
</Dialog.Root>
