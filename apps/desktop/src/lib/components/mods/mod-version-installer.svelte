<script lang="ts">
	import { runtimeErrorMessage } from '#lib/runtime-errors.ts';
	import { localeState } from '#lib/locale.svelte.js';
	import { formatModUpdated } from '../../mod-project-stats';
	import { untrack } from 'svelte';
	import { LoaderCircle, Check, Download, ChevronUp, RefreshCw } from '@lucide/svelte';
	import { Portal } from 'bits-ui';
	import { modPlanWarnings } from '../../mod-plan-warnings';
	import { ModCatalogState } from '../../mod-catalog-state.svelte';
	import { ModInstallPlanState, type ModPlanSnapshot, type ModInstallFile } from '../../mod-install-plan';
	import { getResourceInstallState } from '../../resource-install-state.svelte';
	import { formatBytes as downloadBytes } from '../../download-progress';
	function formatBytes(value: number) { return downloadBytes(value, localeState.current); }
	import { listModrinthFiles, listCurseForgeFiles, createModrinthInstallPlan, createCurseForgeInstallPlan,
		type ModSource, type CurseForgePage } from '../../runtime';
	let { source, name, gameVersion, project, footerTarget, blocked = false }: { source: ModSource; name: string; gameVersion: string; project: string; footerTarget: HTMLElement | undefined; blocked?: boolean } = $props();
	const resources = getResourceInstallState();
	const files = new ModCatalogState<CurseForgePage<ModInstallFile>>();
	let index = $state(0), history = $state<number[]>([]);
	let review = $state<ModPlanSnapshot>({ selected: null, plan: null, busy: false, error: null, expired: false, accepted: false });
	const plans = new ModInstallPlanState({
		read: async (context, file, signal) => {
			if (context.source === 'modrinth' && typeof file.id === 'string') return createModrinthInstallPlan(context.name, context.project, file.id, signal);
			if (context.source === 'curseforge' && typeof file.id === 'number') return createCurseForgeInstallPlan(context.name, Number(context.project), file.id, signal);
			throw new Error(localeState.messages.mods.discovery.sourceMismatch);
		},
		start: (target, id) => resources.start(target, id), canSubmit: id => !blocked && resources.canSubmit(id),
	}, value => { review = value; });
	const button = 'inline-flex items-center justify-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	const reused = $derived(review.plan?.files.filter(file => file.reuse).length ?? 0);
	const operation = $derived(resources.operation?.id === review.plan?.planId ? resources.operation : null);
	const accepted = $derived(review.accepted || operation !== null);
	const planLocked = $derived(resources.starting || (resources.uncertain && resources.intent !== null));
	const dependencies = $derived(review.plan?.items.filter(item => !item.direct).length ?? 0);
	const warnings = $derived(modPlanWarnings(review.plan?.warnings ?? [], review.plan?.warningDetails, localeState.current));
	let detailsOpen = $state(false);
	const detailsId = $props.id();
	$effect(() => { review.selected?.id; review.plan?.planId; detailsOpen = false; });
	function load(force = false) {
		const current = source, target = name, id = project, offset = index;
		return files.load((signal, options) => current === 'modrinth' ? listModrinthFiles(target, id, offset, signal, options) : listCurseForgeFiles(target, Number(id), offset, signal, options), force);
	}
	$effect(() => {
		const context = { source, name, gameVersion, project };
		untrack(() => { index = 0; history = []; plans.bind(context); });
		return () => plans.dispose();
	});
	$effect(() => {
		source; name; gameVersion; project; index;
		untrack(() => { plans.clear(); files.clear(); void load(); });
		return () => files.cancel();
	});
	function downloads() { document.querySelector<HTMLButtonElement>('[data-download-tasks-trigger]')?.click(); }
</script>

<div class="flex min-h-0 flex-1 flex-col">
	{#if files.busy}<p role="status" class="mt-3 flex items-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.mods.discovery.loadingVersions}</p>{/if}
	{#if files.error || files.value?.refreshError}<p role="alert" class="mt-3 text-xs text-destructive">{runtimeErrorMessage(files.errorCode ?? files.value?.refreshErrorCode, localeState.current, localeState.messages.mods.discovery.catalogFailed)}{files.value ? localeState.messages.mods.discovery.keepVersions : ''}<button type="button" class={button} disabled={files.busy || planLocked} onclick={() => { plans.clear(); void load(true); }}>{localeState.messages.mods.discovery.retry}</button></p><details class="mt-1 text-[12px] text-muted-foreground"><summary>{localeState.messages.mods.discovery.technicalDetails}</summary><p class="break-words">{files.errorDetail ?? files.value?.refreshError}</p></details>{/if}
	<div class="resource-list mt-1 min-h-0 flex-1 scrollbar-subtle overflow-y-auto overscroll-contain" aria-label={localeState.messages.mods.discovery.chooseVersion}>
		{#each files.value?.items ?? [] as file (file.id)}
			<button type="button" aria-pressed={review.selected?.id === file.id} disabled={blocked || planLocked} onclick={() => void plans.select(file)} class={`resource-row flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring disabled:opacity-40 ${review.selected?.id === file.id ? 'bg-chrome-selected' : 'hover:bg-chrome-hover'}`}>
				<span class="min-w-0 flex-1"><span class="block truncate text-xs font-medium" title={file.displayName}>{file.displayName}</span><span class="mt-1 block truncate text-[12px] text-muted-foreground" title={file.fileName}>{formatModUpdated(file.fileDate, localeState.current) ?? file.fileDate} · {file.releaseType === 1 ? 'Release' : file.releaseType === 2 ? 'Beta' : 'Alpha'} · {formatBytes(file.fileLength)}</span></span>
				{#if review.selected?.id === file.id}<Check size={14} class="shrink-0 text-primary" aria-hidden="true" />{/if}
			</button>
		{:else}{#if !files.busy && !files.error}<p class="px-2 py-5 text-xs text-muted-foreground">{localeState.messages.mods.discovery.noVersions}</p>{/if}{/each}
	</div>
	{#if history.length || files.value?.nextIndex != null}<div class="mt-2 flex justify-end gap-1"><button type="button" class={button} disabled={files.busy || planLocked || !history.length} onclick={() => { index = history.at(-1)!; history = history.slice(0, -1); }}>{localeState.messages.mods.discovery.previousPage}</button><button type="button" class={button} disabled={files.busy || planLocked || files.value?.nextIndex == null} onclick={() => { history = [...history, index]; index = files.value!.nextIndex!; }}>{localeState.messages.mods.discovery.nextPage}</button></div>{/if}
</div>

{#if footerTarget && (review.selected || resources.active || resources.starting || resources.uncertain || resources.error)}
<Portal to={footerTarget}>
	<section aria-label={localeState.messages.mods.discovery.confirmPlan} class="bg-card">
		{#if review.plan && detailsOpen}
			<div id={detailsId} class="max-h-[min(35dvh,16rem)] scrollbar-subtle overflow-y-auto overscroll-contain border-b border-border px-4 py-2.5">
				<ul class="divide-y divide-border">{#each review.plan.items as item (item.project.id)}
					<li class="flex items-center gap-2 py-2"><div class="min-w-0 flex-1"><p class="truncate text-xs" title={item.project.name}>{item.project.name}<span class="ml-2 text-[12px] text-muted-foreground">{item.direct ? localeState.messages.mods.discovery.selectedMod : localeState.messages.mods.discovery.required}</span></p><p class="mt-0.5 truncate text-[12px] text-muted-foreground" title={item.file.fileName}>{item.file.displayName}</p></div><span class="shrink-0 text-right text-[12px] leading-4 text-muted-foreground">{review.plan.files.find(file => file.projectId === item.project.id && file.fileId === item.file.id)?.reuse ? localeState.messages.mods.discovery.reuse : localeState.messages.mods.discovery.download}<br />{formatBytes(item.file.fileLength)}</span></li>
				{/each}</ul>
				<p class="mt-2 text-[12px] text-muted-foreground">{localeState.messages.mods.planFiles(review.plan.files.length, reused)}</p>
				{#if review.plan.optionalProjects.length}<p class="mt-2 text-[12px] leading-5 text-muted-foreground">{localeState.messages.mods.optionalExcluded(review.plan.optionalProjects.length)}</p>{/if}
				{#each warnings.details as warning}<p class="mt-2 text-[12px] leading-5 text-muted-foreground">{warning}</p>{/each}
			</div>
		{/if}
		<div class="flex flex-wrap items-center gap-x-2 gap-y-2 px-3 py-2.5">
			<div class="min-w-0 flex-1 text-xs">
				{#if review.busy}<p role="status" class="flex items-center gap-2 text-muted-foreground"><LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.mods.discovery.preparingPlan}</p>
				{:else if review.error}<p role="alert" class="break-words text-destructive">{runtimeErrorMessage(review.errorCode, localeState.current, localeState.messages.mods.discovery.planFailed)}</p><details class="mt-1 text-[12px] text-muted-foreground"><summary>{localeState.messages.mods.discovery.technicalDetails}</summary><p class="break-words">{review.error}</p></details>
				{:else if accepted}<p role="status" class={operation?.state === 'failed' ? 'text-destructive' : 'text-muted-foreground'}>{operation?.state === 'completed' ? localeState.messages.mods.discovery.installed : operation?.state === 'cancelled' ? localeState.messages.mods.discovery.cancelled : operation?.state === 'failed' ? localeState.messages.mods.discovery.failed : localeState.messages.mods.discovery.submitted}</p>
				{:else if review.expired}<p role="status" class="text-destructive">{localeState.messages.mods.discovery.planExpired}</p>
				{:else if review.plan}<p class="truncate" title={localeState.messages.mods.installSummary(name, formatBytes(review.plan.downloadBytes))}><span class="text-muted-foreground">{localeState.messages.mods.discovery.installTo}</span> {name} <span class="text-muted-foreground">{localeState.messages.mods.downloadSize(formatBytes(review.plan.downloadBytes))}</span></p>{#if dependencies}<p class="mt-0.5 text-[12px] text-muted-foreground">{localeState.messages.mods.requiredCount(dependencies)}</p>{/if}
				{:else if resources.active}<p class="truncate text-muted-foreground">{localeState.messages.mods.installingFor(resources.operation?.name ?? '')}</p>{/if}
			</div>
			{#if review.plan}<button type="button" class={`${button} size-8 shrink-0 px-0`} aria-label={detailsOpen ? localeState.messages.mods.discovery.collapseDetails : localeState.messages.mods.discovery.expandDetails} title={detailsOpen ? localeState.messages.mods.discovery.collapseDetails : localeState.messages.mods.discovery.dependencyDetails} aria-expanded={detailsOpen} aria-controls={detailsId} onclick={() => detailsOpen = !detailsOpen}><ChevronUp size={15} class={detailsOpen ? 'rotate-180' : ''} aria-hidden="true" /></button>{/if}
			{#if accepted}
				{#if operation?.state === 'failed' || operation?.state === 'cancelled'}<button type="button" class={button} disabled={blocked || planLocked} onclick={() => review.selected && void plans.select(review.selected)}>{localeState.messages.mods.discovery.regenerate}</button>{/if}<button type="button" class={button} onclick={downloads}>{localeState.messages.mods.discovery.viewTasks}</button>
			{:else if review.error || review.expired}<button type="button" class={button} disabled={blocked || review.busy || planLocked || (resources.uncertain && resources.intent !== null)} onclick={() => review.selected && void plans.select(review.selected)}><RefreshCw size={13} aria-hidden="true" />{localeState.messages.mods.discovery.regenerate}</button>
			{:else if review.plan}<button type="button" disabled={blocked || review.busy || !resources.canSubmit(review.plan.planId)} onclick={() => void plans.confirm()} class="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Download size={13} aria-hidden="true" />{resources.starting ? localeState.messages.mods.discovery.submitting : resources.uncertain && resources.intent?.id === review.plan.planId ? localeState.messages.mods.discovery.retrySubmit : localeState.messages.mods.discovery.confirmInstall}</button>{/if}
		</div>
		{#if warnings.technical.length}<details class="px-3 pb-2 text-[12px] text-muted-foreground"><summary>{localeState.messages.mods.discovery.technicalDetails}</summary>{#each warnings.technical as detail}<p class="mt-1 break-words">{detail}</p>{/each}</details>{/if}
		{#if warnings.important.length}<div role="alert" class="px-3 pb-2.5 text-[12px] leading-5 text-destructive">{#each warnings.important as warning}<p>{warning}</p>{/each}</div>{/if}
		{#if resources.errorDetail}<details class="px-3 py-2 text-[12px] text-muted-foreground"><summary>{localeState.messages.mods.discovery.technicalDetails}</summary><p class="break-words">{resources.errorDetail}</p></details>{/if}
		{#if resources.error || (resources.active && !accepted) || resources.uncertain}
			<div class="flex flex-wrap items-center gap-x-2 gap-y-1 border-t border-border px-3 py-2 text-[12px] text-muted-foreground" aria-label={localeState.messages.mods.discovery.windowTask}>
				<p class="min-w-0 flex-1 break-words">{resources.error ?? (resources.active ? localeState.messages.mods.waitingFor(resources.operation?.name ?? '') : localeState.messages.mods.discovery.confirmingTask)}</p>
				{#if resources.error && review.plan && !accepted && !resources.uncertain && !resources.active && !review.expired}<button type="button" class={button} disabled={blocked || review.busy || planLocked} onclick={() => review.selected && void plans.select(review.selected)}>{localeState.messages.mods.discovery.regenerate}</button>{/if}
				<button type="button" class={button} onclick={downloads}>{localeState.messages.mods.discovery.viewTasks}</button>{#if resources.uncertain}<button type="button" class={button} disabled={resources.syncing || resources.starting} onclick={() => void resources.sync()}>{localeState.messages.mods.discovery.refreshStatus}</button>{/if}
			</div>
		{/if}
	</section>
</Portal>
{/if}
