<script lang="ts">
	import { Box } from '@lucide/svelte';
	import type { InstalledClient } from '../../runtime';
	let { client, onInspect }: { client: InstalledClient; onInspect: (id: string) => void } = $props();
</script>
<li class="min-w-0">
<button type="button" data-client-details-trigger={client.id} aria-label={`查看 ${client.name} 的实例详情`} onclick={() => onInspect(client.id)} class="flex w-full min-w-0 items-center gap-3 rounded-xl border border-border bg-card px-4 py-3.5 text-left transition-colors hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">
	<span class="grid size-10 shrink-0 place-items-center rounded-lg bg-primary/8 text-primary"><Box size={21} strokeWidth={1.5} aria-hidden="true" /></span>
	<span class="min-w-0 flex-1"><span class="block truncate text-[13px] font-medium" title={client.name}>{client.name}</span><span class="mt-1 block truncate text-[11px] text-muted-foreground">Minecraft Java · {client.versionId}</span></span>
	<span class="shrink-0 rounded-md bg-chrome-hover px-2 py-1 text-[10px] text-muted-foreground">已安装</span>
</button>
</li>
