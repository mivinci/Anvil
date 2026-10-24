<script lang="ts">
	import { Download, Clock3 } from '@lucide/svelte';
	import type { ModProjectSummary } from '../../runtime';
	import { formatModDownloads, formatModUpdated } from '../../mod-project-stats';
	let { project, compact = false }: { project: ModProjectSummary; compact?: boolean } = $props();
	const downloads = $derived(formatModDownloads(project.downloadCount));
	const updated = $derived(formatModUpdated(project.updatedAt ?? project.dateModified));
	const downloadLabel = $derived(downloads !== null ? `平台累计下载量：${project.downloadCount!.toLocaleString('zh-CN')}` : '');
</script>

{#if downloads !== null || updated}
	<span class={`flex shrink-0 text-[10px] leading-4 text-muted-foreground tabular-nums ${compact ? 'flex-col items-end gap-0.5' : 'mt-3 flex-wrap items-center gap-x-4 gap-y-1 text-[11px]'}`}>
		{#if downloads !== null}<span class="inline-flex items-center gap-1 whitespace-nowrap" title={downloadLabel} aria-label={downloadLabel}><Download size={11} strokeWidth={1.6} aria-hidden="true" />{downloads}</span>{/if}
		{#if updated}<span class="inline-flex items-center gap-1 whitespace-nowrap" title={`最近更新（UTC）：${project.updatedAt ?? project.dateModified}`} aria-label={`最近更新：${updated}`}><Clock3 size={11} strokeWidth={1.6} aria-hidden="true" /><time datetime={project.updatedAt ?? project.dateModified ?? ''}>{updated}</time></span>{/if}
	</span>
{/if}
