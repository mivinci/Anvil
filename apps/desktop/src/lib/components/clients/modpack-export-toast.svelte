<script lang="ts">
	import { Check, Copy, X } from '@lucide/svelte';
	import { formatBytes } from '../../download-progress';
	import { getModpackExportState } from '../../modpack-export-state.svelte.js';
	const exports = getModpackExportState();
	let dismissed = $state<number | null>(null);
	let copied = $state(false);
	let copyError = $state(false);
	let timer: ReturnType<typeof setTimeout> | undefined;
	const operation = $derived(exports.operation);
	const visible = $derived(operation?.state === 'completed' && !!operation.result && dismissed !== operation.id);
	$effect(() => {
		if (!visible || !operation) return;
		const id = operation.id;
		timer = setTimeout(() => { dismissed = id; }, 10_000);
		return () => { if (timer) clearTimeout(timer); };
	});
	async function copyPath() {
		if (!operation?.result) return;
		copied = false; copyError = false;
		try { await navigator.clipboard.writeText(operation.result.path); copied = true; }
		catch { copyError = true; }
	}
</script>

{#if visible && operation?.result}
	<div role="status" class="fixed right-4 bottom-4 z-50 w-[min(360px,calc(100vw-32px))] rounded-xl bg-popover p-3 text-popover-foreground shadow-xl">
		<div class="flex items-start gap-2">
			<span class="mt-0.5 grid size-5 shrink-0 place-items-center rounded-full bg-primary/10 text-primary"><Check size={12} strokeWidth={2.5} aria-hidden="true" /></span>
			<div class="min-w-0 flex-1">
				<p class="text-xs font-medium">整合包导出完成</p>
				<p class="mt-1 break-all text-[11px] leading-4 text-muted-foreground">{operation.result.path}</p>
				<p class="mt-1 text-[10px] text-muted-foreground">{formatBytes(operation.result.size)} · {operation.result.files} 个文件</p>
				<div class="mt-2 flex items-center gap-1.5">
					<button type="button" onclick={() => void copyPath()} class="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring"><Copy size={11} aria-hidden="true" />{copied ? '已复制路径' : copyError ? '复制失败' : '复制路径'}</button>
					<button type="button" onclick={() => { if (operation) dismissed = operation.id; }} class="inline-flex h-7 items-center rounded-md px-2 text-[11px] text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">关闭</button>
				</div>
			</div>
			<button type="button" aria-label="关闭导出通知" title="关闭" onclick={() => { if (operation) dismissed = operation.id; }} class="grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring"><X size={13} aria-hidden="true" /></button>
		</div>
	</div>
{/if}
