<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { validClientName } from '../../client-name.ts';
	import { onMount, tick } from 'svelte';
	import { getClientInstallState } from '../../client-install-state.svelte.js';
	const installs = getClientInstallState();
	import { Dialog } from 'bits-ui';
	import ClientLoaderPicker, { type ClientLoader } from './client-loader-picker.svelte';
	let loader = $state<ClientLoader>('vanilla');
	let loaderVersion = $state<string | null>(null);
	const loaderReady = $derived(loader === 'vanilla' || (loader === 'fabric' && loaderVersion !== null));
	const loaderName = $derived(({ vanilla: localeState.messages.clients.flow.vanilla, fabric: 'Fabric', neoforge: 'NeoForge', forge: 'Forge' })[loader]);
	import SelectableRow from '../ui/selectable-row.svelte';
	import SelectableList from '../ui/selectable-list.svelte';
	import { LoaderCircle, RefreshCw, Search, X } from '@lucide/svelte';
	import { type GameVersion, type InstalledClient } from '../../runtime';
	let { clients, onClose }: { clients: InstalledClient[]; onClose: () => void } = $props();
	import { getClientVersionState } from '../../client-version-state.svelte.js';
	const catalog = getClientVersionState();
	const versions = $derived(catalog.versions);
	const loading = $derived(catalog.loading);
	const error = $derived(catalog.error);
	let step = $state(1);
	let body = $state<HTMLDivElement>();
	const steps = $derived([localeState.messages.clients.flow.chooseVersion, localeState.messages.clients.flow.chooseLoader, localeState.messages.clients.flow.confirmInstance]);
	const panelClass = 'col-start-1 row-start-1 flex min-h-0 flex-col overflow-y-auto overscroll-contain px-5 pb-4 pt-2 transition-[opacity,transform] duration-200 ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none';
	function position(index: number) { return step === index ? 'translate-x-0 opacity-100' : step > index ? 'pointer-events-none -translate-x-4 opacity-0' : 'pointer-events-none translate-x-4 opacity-0'; }
	async function move(next: number) {
		if (next < 1 || next > 3) return;
		if (next > step && ((step === 1 && (!selected || !catalog.loaded)) || (step === 2 && !loaderReady))) return;
		step = next;
		if (next === 3) touched = true;
		await tick();
		body?.querySelector<HTMLElement>(`[data-install-step="${step}"] h2`)?.focus({ preventScroll: true });
	}
	let channel = $state<'release' | 'snapshot'>('release');
	let query = $state('');
	let selected = $state<GameVersion | null>(null);
	let name = $state('');
	let nameEdited = $state(false);
	let touched = $state(false);
	const visible = $derived(versions.filter((version) => version.type === channel && version.id.toLowerCase().includes(query.trim().toLowerCase())));
	const nameError = $derived(!name ? localeState.messages.clients.flow.nameRequired : !validClientName(name) ? localeState.messages.clients.flow.nameInvalid : clients.some((client) => client.name.toLowerCase() === name.toLowerCase()) ? localeState.messages.clients.flow.duplicateName : null);
	function select(version: GameVersion) {
		selected = version;
	}
	$effect(() => {
		if (selected && selected.type === channel && versions.some((version) => version.id === selected?.id)) return;
		const first = versions.find((version) => version.type === channel);
		if (first) select(first);
		else selected = null;
	});
	$effect(() => {
		if (nameEdited || !selected) return;
		const suffix = loader === 'vanilla' ? '' : `-${loader}`;
		name = selected.id.replace(/[^a-zA-Z0-9._-]/g, '-').slice(0, 48 - suffix.length).replace(/\.+$/, '') + suffix;
	});
	async function submit() {
		touched = true;
		if (step !== 3 || !selected || nameError || !installs.canStart || !catalog.loaded || !loaderReady) return;
		void installs.start(name, selected.id, loader === 'fabric' && loaderVersion ? { kind: 'fabric', version: loaderVersion } : null);
		onClose();
		await tick();
		installs.detailsOpen = true;
	}
	onMount(() => { void catalog.load(); });
</script>

<Dialog.Root open onOpenChange={(open) => { if (!open) onClose(); }}>
	<Dialog.Overlay class="fixed inset-0 z-40 bg-black/15 backdrop-blur-[2px]" />
	<Dialog.Content class="fixed top-1/2 left-1/2 z-50 flex h-128 max-h-[calc(100dvh-48px)] w-135 max-w-[calc(100vw-48px)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-border bg-popover text-popover-foreground shadow-[0_12px_48px_rgb(0_0_0/0.16)] outline-none" onInteractOutside={(event) => event.preventDefault()} onCloseAutoFocus={(event) => { event.preventDefault(); document.querySelector<HTMLButtonElement>('[data-client-install-trigger]')?.focus(); }}>
		<header class="shrink-0 px-5 pt-5 pb-4"><Dialog.Title class="text-base font-semibold">{localeState.messages.clients.flow.installMinecraft}</Dialog.Title><Dialog.Description class="mt-1 text-xs text-muted-foreground">{localeState.messages.clients.flow.installDescription}</Dialog.Description><button type="button" aria-label={localeState.messages.clients.flow.closeInstaller} onclick={onClose} class="absolute top-4 right-4 grid size-6 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring"><X size={14} aria-hidden="true" /></button></header>
		<nav aria-label={localeState.messages.clients.flow.installSteps} class="flex shrink-0 items-center gap-3 px-5 pb-3">
			{#each steps as label, index}
				{#if index > 0}<span aria-hidden="true" class="h-px w-5 bg-border"></span>{/if}
				<button type="button" aria-current={step === index + 1 ? 'step' : undefined} disabled={index + 1 > step} onclick={() => void move(index + 1)} class={`flex items-center gap-1.5 rounded-md text-xs focus-visible:outline-2 focus-visible:outline-ring ${step === index + 1 ? 'text-foreground' : 'text-muted-foreground'}`}><span class={`grid size-5 place-items-center rounded-full text-xs tabular-nums ${step === index + 1 ? 'bg-chrome-selected font-medium text-accent-foreground' : 'bg-chrome-hover'}`}>{index + 1}</span>{label}</button>
			{/each}
		</nav>
		<div bind:this={body} class="relative grid min-h-0 flex-1 overflow-hidden">
		<section data-install-step="1" aria-hidden={step !== 1} inert={step !== 1} class={`${panelClass} ${position(1)}`}>
			<h2 tabindex="-1" class="sr-only">{localeState.messages.clients.flow.chooseGame}</h2>
			<div class="flex items-center justify-between gap-3"><div role="group" aria-label={localeState.messages.clients.flow.versionType} class="inline-flex rounded-[10px] bg-chrome-hover p-0.75">{#each [{ value: 'release', label: localeState.messages.clients.flow.release }, { value: 'snapshot', label: localeState.messages.clients.flow.snapshot }] as option}<button type="button" aria-pressed={channel === option.value} onclick={() => { channel = option.value as 'release' | 'snapshot'; }} class={`rounded-[7px] px-3 py-1 text-xs focus-visible:outline-2 focus-visible:outline-ring ${channel === option.value ? 'bg-chrome-selected text-accent-foreground' : 'text-muted-foreground'}`}>{option.label}</button>{/each}</div><div class="flex items-center gap-1"><button type="button" aria-label={localeState.messages.clients.flow.refreshVersions} title={localeState.messages.clients.flow.refreshVersions} disabled={loading} onclick={() => void catalog.load(true)} class="grid size-8 shrink-0 place-items-center rounded-lg text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={13} class={loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button><label class="flex h-8 w-44 items-center gap-2 rounded-lg bg-chrome-hover px-2.5 text-muted-foreground focus-within:ring-1 focus-within:ring-ring"><Search size={13} aria-hidden="true" /><input type="search" aria-label={localeState.messages.clients.flow.searchMinecraft} placeholder={localeState.messages.clients.flow.searchVersion} bind:value={query} class="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground" /></label></div></div>
			<div class="mt-3 min-h-0 flex-1 overflow-y-auto overscroll-contain rounded-lg border border-border" aria-busy={loading}>
				{#if !catalog.loaded && loading}<div role="status" class="flex h-full items-center justify-center gap-2 text-xs text-muted-foreground"><LoaderCircle size={15} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{localeState.messages.clients.flow.loadingCatalog}</div>
				{:else if !catalog.loaded && error}<div class="p-4 text-xs" role="alert"><p class="text-destructive">{localeState.messages.clients.flow.versionsFailed}</p><button type="button" onclick={() => void catalog.load(true)} class="mt-2 rounded px-2 py-1 text-primary hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">{localeState.messages.clients.flow.fetchAgain}</button><details class="mt-2 text-xs text-muted-foreground"><summary>{localeState.messages.clients.flow.errorDetails}</summary><p class="mt-1 select-text wrap-break-word">{error}</p></details></div>
				{:else if visible.length === 0}<p class="flex h-full items-center justify-center text-xs text-muted-foreground">{query ? localeState.messages.clients.flow.noVersionMatches : localeState.messages.clients.flow.noChannelVersions}</p>
				{:else}
					<SelectableList role="group" aria-label={localeState.messages.clients.flow.availableVersions}>
						{#each visible as version (version.id)}
							<SelectableRow selected={selected?.id === version.id} aria-pressed={selected?.id === version.id} onclick={() => select(version)}>
								<span class="truncate">{version.id}</span>
								{#snippet trailing()}{version.releaseTime.slice(0, 10)}{/snippet}
							</SelectableRow>
						{/each}
					</SelectableList>
				{/if}
			</div>
			{#if catalog.loaded && error}<p role="alert" class="mt-2 text-xs leading-4 text-destructive">{localeState.messages.clients.flow.versionsKeep}<button type="button" disabled={loading} onclick={() => void catalog.load(true)} class="ml-1 underline underline-offset-2">{localeState.messages.clients.flow.retry}</button></p>{/if}
		</section>
		<section data-install-step="2" aria-hidden={step !== 2} inert={step !== 2} class={`${panelClass} ${position(2)}`}>
			<h2 tabindex="-1" class="sr-only">{localeState.messages.clients.flow.chooseLoader}</h2>
			<p class="text-xs text-muted-foreground">Minecraft {selected?.id ?? '—'}</p>
			<ClientLoaderPicker bind:value={loader} bind:version={loaderVersion} gameVersion={selected?.id ?? null} />
		</section>
		<section data-install-step="3" aria-hidden={step !== 3} inert={step !== 3} class={`${panelClass} ${position(3)}`}>
			<h2 tabindex="-1" class="sr-only">{localeState.messages.clients.flow.confirmInstance}</h2>
			<dl class="shrink-0 divide-y divide-border rounded-lg border border-border bg-card px-3">
				<div class="flex items-center justify-between gap-3 py-2.5 text-xs"><dt class="text-muted-foreground">{localeState.messages.clients.flow.gameVersion}</dt><dd>Minecraft {selected?.id ?? '—'}</dd></div>
				<div class="flex items-center justify-between gap-3 py-2.5 text-xs"><dt class="text-muted-foreground">{localeState.messages.clients.flow.loader}</dt><dd>{loaderName}{loader === 'fabric' && loaderVersion ? ` ${loaderVersion}` : ''}</dd></div>
			</dl>
			<div class="mt-4"><label for="client-instance-name" class="text-body font-medium">{localeState.messages.clients.flow.instanceName}</label><input id="client-instance-name" value={name} oninput={(event) => { name = event.currentTarget.value; nameEdited = true; touched = true; }} onblur={() => touched = true} maxlength={48} autocomplete="off" spellcheck="false" aria-invalid={touched && nameError !== null} aria-describedby="client-name-hint" placeholder={localeState.messages.clients.flow.nameExample} class="mt-2 h-8 w-full rounded-lg border border-input bg-main-pane px-2.5 text-body outline-none focus:ring-1 focus:ring-ring" /><p id="client-name-hint" class={`mt-1.5 text-xs leading-4 ${touched && nameError ? 'text-destructive' : 'text-muted-foreground'}`}>{touched && nameError ? nameError : localeState.messages.clients.flow.nameHint}</p></div>
			{#if !installs.canStart}<div class="mt-3 text-xs text-muted-foreground">{installs.active ? localeState.messages.clients.flow.installBusy : installs.starting ? localeState.messages.clients.flow.creatingTaskNotice : localeState.messages.clients.flow.checkingTask}<button type="button" onclick={() => { onClose(); installs.detailsOpen = true; }} class="ml-1 rounded px-1 py-0.5 text-primary hover:bg-chrome-hover">{localeState.messages.clients.flow.viewTask}</button></div>{/if}
		</section>
		</div>
		<footer class="flex shrink-0 items-center justify-between gap-3 border-t border-border px-5 py-3">
			<span class="min-w-0 truncate text-xs text-muted-foreground">{step} / 3{selected ? ` · Minecraft ${selected.id}` : ''}</span>
			<div class="flex shrink-0 gap-2">
				<button type="button" onclick={() => step === 1 ? onClose() : void move(step - 1)} class="h-8 rounded-lg px-3 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">{step === 1 ? localeState.messages.clients.flow.close : localeState.messages.clients.flow.previous}</button>
				{#if step < 3}<button type="button" onclick={() => void move(step + 1)} disabled={step === 1 ? !selected || !catalog.loaded : !selected || !loaderReady} class="h-8 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{localeState.messages.clients.flow.next}</button>
				{:else}<button type="button" onclick={submit} disabled={!selected || !!nameError || !installs.canStart || !catalog.loaded || !loaderReady} class="h-8 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{localeState.messages.clients.flow.startInstall}</button>{/if}
			</div>
		</footer>
	</Dialog.Content>
</Dialog.Root>
