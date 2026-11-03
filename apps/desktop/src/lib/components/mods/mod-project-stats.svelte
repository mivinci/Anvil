<script lang="ts">
	import { localeState } from '#lib/locale.svelte.js';
	import { Download, Clock3 } from '@lucide/svelte';
	import type { ModProjectSummary } from '../../runtime';
	import { formatModDownloads, formatModUpdated } from '../../mod-project-stats';
	let { project, compact = false }: { project: ModProjectSummary; compact?: boolean } = $props();
	const downloads = $derived(formatModDownloads(project.downloadCount, localeState.current));
	const updated = $derived(formatModUpdated(project.updatedAt ?? project.dateModified, localeState.current));
	const downloadLabel = $derived(downloads !== null ? localeState.messages.mods.downloadsLabel(project.downloadCount!) : '');
</script>

{#if downloads !== null || updated}
	<span class={`flex shrink-0 text-xs leading-4 text-muted-foreground tabular-nums ${compact ? 'flex-col items-end gap-0.5' : 'mt-3 flex-wrap items-center gap-x-4 gap-y-1 text-xs'}`}>
		{#if downloads !== null}<span class="inline-flex items-center gap-1 whitespace-nowrap" title={downloadLabel} aria-label={downloadLabel}><Download size={11} strokeWidth={1.6} aria-hidden="true" />{downloads}</span>{/if}
		{#if updated}<span class="inline-flex items-center gap-1 whitespace-nowrap" title={localeState.messages.mods.updatedUtc(project.updatedAt ?? project.dateModified ?? '')} aria-label={localeState.messages.mods.updatedLabel(updated)}><Clock3 size={11} strokeWidth={1.6} aria-hidden="true" /><time datetime={project.updatedAt ?? project.dateModified ?? ''}>{updated}</time></span>{/if}
	</span>
{/if}
