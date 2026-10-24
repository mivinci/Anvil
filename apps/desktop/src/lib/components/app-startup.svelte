<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	import { isTauri } from '@tauri-apps/api/core';
	import { LoaderCircle } from '@lucide/svelte';
	import { shell } from '../shell';
	import { getRuntimeHealth } from '../runtime';
	let { children }: { children: Snippet } = $props();
	let ready = $state(false);
	let error = $state<string | null>(null);
	let message = $state('正在连接 Runtime…');
	let logsError = $state(false);
	onMount(() => {
		document.getElementById('app-startup')?.remove();
		if (!isTauri()) { ready = true; return; }
		const cancel = new AbortController();
		let timer: ReturnType<typeof setTimeout>;
		const deadline = setTimeout(() => cancel.abort(), 30_000);
		async function connect() {
			try {
				while (!cancel.signal.aborted) {
					const status = await shell.getRuntimeStatus();
					if (cancel.signal.aborted) return;
					if (status.state === 'ready') {
						message = '正在准备界面…';
						await getRuntimeHealth(AbortSignal.any([cancel.signal, AbortSignal.timeout(5000)]));
						if (!cancel.signal.aborted) ready = true;
						return;
					}
					if (['failed', 'stopped', 'not_installed'].includes(status.state)) throw new Error('runtime_unavailable');
					await new Promise<void>(resolve => { timer = setTimeout(resolve, 200); });
				}
			} catch {
				if (!cancel.signal.aborted) error = '无法连接 Runtime，请重试或查看日志。';
			} finally { clearTimeout(deadline); }
		}
		cancel.signal.addEventListener('abort', () => {
			error = '连接 Runtime 超时，请重试或查看日志。';
		}, { once: true });
		void connect();
		return () => { clearTimeout(deadline); clearTimeout(timer); cancel.abort(); };
	});
	async function openLogs() {
		try { await shell.openLogDirectory(); } catch { logsError = true; }
	}
</script>

{#if ready}
	{@render children()}
{:else}
	<header class="h-8 shrink-0" data-tauri-drag-region aria-label="窗口拖动区域"></header>
	<div class="grid min-h-0 flex-1 place-content-center gap-3 px-6 pb-8 text-center">
		<p class="text-lg font-semibold text-foreground">Anvil</p>
		{#if error}
			<p role="alert" class="text-xs text-muted-foreground">{error}</p>
			<div class="flex justify-center gap-2">
				<button type="button" class="rounded-lg bg-chrome-selected px-3 py-1.5 text-xs focus-visible:outline-2 focus-visible:outline-ring" onclick={() => location.reload()}>重试</button>
				<button type="button" class="rounded-lg px-3 py-1.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring" onclick={() => void openLogs()}>查看日志</button>
			</div>
			{#if logsError}<p role="alert" class="text-xs text-muted-foreground">无法打开日志目录。</p>{/if}
		{:else}
			<p role="status" class="flex items-center justify-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{message}</p>
		{/if}
	</div>
{/if}
