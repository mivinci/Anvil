<script lang="ts">
	import { tick } from 'svelte';
	import { Download } from '@lucide/svelte';
	import { Popover } from 'bits-ui';
	import { getClientInstallState } from '../client-install-state.svelte.js';
	import ClientInstallProgress from './clients/client-install-progress.svelte';
	import { getResourceInstallState } from '../resource-install-state.svelte.js';
	import ResourceInstallProgress from './clients/resource-install-progress.svelte';
	const resources = getResourceInstallState();
	const installs = getClientInstallState();
	let open = $state(false);
</script>

<Popover.Root bind:open>
	<Popover.Trigger data-download-tasks-trigger class="relative grid size-6 cursor-default place-items-center rounded-md text-chrome-muted/80 transition-colors hover:bg-chrome-selected hover:text-chrome-muted focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring data-[state=open]:bg-chrome-selected" aria-label={resources.active || resources.starting ? `下载任务：${resources.label}` : installs.active || installs.starting ? `下载任务：${installs.label}` : '下载任务'} title="下载任务">
		<Download size={16} aria-hidden="true" />{#if installs.active || installs.starting || resources.active || resources.starting}<span class="absolute top-0 right-0 size-1.5 rounded-full bg-primary" aria-hidden="true"></span>{/if}
	</Popover.Trigger>
	<Popover.Content side="bottom" align="end" sideOffset={8} collisionPadding={8} class="z-30 w-72 cursor-default rounded-xl border border-chrome-control-border bg-popover p-4 text-popover-foreground shadow-lg outline-none" aria-label="下载任务">
		<h2 class="mb-4 text-sm font-semibold">下载任务</h2>
		{#if installs.operation || installs.starting || installs.error}<ClientInstallProgress onNavigate={() => open = false} /><button type="button" onclick={async () => { open = false; await tick(); installs.detailsOpen = true; }} class="mt-2 w-full rounded-md py-1.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">查看详情</button>
		{:else if !resources.operation && !resources.error && !resources.starting}<div class="flex flex-col items-center gap-3 py-6 text-chrome-muted"><Download size={24} strokeWidth={1.5} aria-hidden="true" /><p class="text-xs">{installs.syncing ? '正在读取任务…' : '暂无下载任务'}</p></div>{/if}
		{#if resources.operation || resources.error || resources.starting}<div class={installs.operation || installs.error ? 'mt-3 border-t border-border pt-3' : ''}><ResourceInstallProgress /></div>{/if}
	</Popover.Content>
</Popover.Root>
