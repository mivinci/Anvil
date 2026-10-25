<script lang="ts">
	import { Dialog } from 'bits-ui';
	import { X } from '@lucide/svelte';
	import { getClientInstallState } from '../../client-install-state.svelte.js';
	import ClientInstallProgress from './client-install-progress.svelte';
	const installs = getClientInstallState();
</script>
<Dialog.Root open={installs.detailsOpen} onOpenChange={(open) => installs.detailsOpen = open}>
	<Dialog.Overlay class="fixed inset-0 z-40 bg-black/15 backdrop-blur-[2px]" />
	<Dialog.Content class="fixed top-1/2 left-1/2 z-50 max-h-[calc(100dvh-48px)] w-96 max-w-[calc(100vw-48px)] -translate-x-1/2 -translate-y-1/2 overflow-y-auto rounded-2xl border border-border bg-popover p-4 text-popover-foreground shadow-[0_12px_48px_rgb(0_0_0/0.16)] outline-none" onCloseAutoFocus={(event) => { event.preventDefault(); document.querySelector<HTMLButtonElement>('[data-download-tasks-trigger]')?.focus(); }}>
		<Dialog.Title class="pr-6 text-sm font-semibold">安装任务</Dialog.Title>
		<Dialog.Description class="mt-1 text-[11px] leading-5 text-muted-foreground">关闭面板不影响任务，可从右上角下载任务继续查看。</Dialog.Description>
		<button type="button" aria-label="关闭安装进度" onclick={() => installs.detailsOpen = false} class="absolute top-4 right-4 grid size-6 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring"><X size={14} aria-hidden="true" /></button>
		<div class="mt-3"><ClientInstallProgress onClose={() => installs.detailsOpen = false} closeLabel={installs.active || installs.starting ? '收起' : '关闭'} /></div>
	</Dialog.Content>
</Dialog.Root>
