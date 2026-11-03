<script lang="ts">
	import { localeState } from '#lib/locale.svelte.ts';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { untrack } from 'svelte';
	import { listClientResources } from '#lib/runtime.js';
	import { validateClientResourcePacks } from '#lib/client-resource-pack-state.svelte.js';
	import { validateShaderPackChange } from '#lib/shader-packs.js';
	import ClientScreenPreview from '#lib/components/clients/client-screen-preview.svelte';
	import ClientLaunchControls from '#lib/components/clients/client-launch-controls.svelte';
	import ClientModpackExport from '#lib/components/clients/client-modpack-export.svelte';
	import ClientAvatarEditor from '#lib/components/clients/client-avatar-editor.svelte';
	import { getClientLaunchState, launchActive, launchLabel } from '#lib/client-launch-state.svelte.js';
	import { Boxes, Image, Sparkles, ArrowUpRight, ChevronDown, Check, Copy } from '@lucide/svelte';
	import { getClientDetailState } from '#lib/client-detail-state.svelte.js';
	const detail = getClientDetailState();
	const name = $derived(page.params.name ?? '');
	let packCount = $state<number | null>(null);
	let shaderCount = $state<number | null>(null);
	let modCount = $state<number | null>(null);
	let countLoading = $state(false);
	let countError = $state(false);
	let countRequest: AbortController | null = null;
	async function loadCount(target: string) {
		countRequest?.abort();
		const controller = new AbortController(); countRequest = controller;
		countLoading = true; countError = false;
		try {
			const manifest = await listClientResources(target, AbortSignal.any([controller.signal, AbortSignal.timeout(20_000)]));
			if (![1, 2, 3, 4, 5].includes(manifest.schemaVersion)) throw new Error(localeState.messages.instance.detail.invalidResourceRevision);
			const common = { revision: manifest.revision, appliedRevision: manifest.appliedRevision ?? null };
			const textures = validateClientResourcePacks({ ...common, packs: manifest.resourcePacks ?? [] });
			const shaders = validateShaderPackChange({ ...common, packs: manifest.shaderPacks ?? [] });
			if (countRequest === controller && !controller.signal.aborted) { modCount = manifest.mods.length; packCount = textures.packs.length; shaderCount = shaders.packs.length; }
		} catch {
			if (countRequest === controller && !controller.signal.aborted) { modCount = null; packCount = null; shaderCount = null; countError = true; }
		} finally {
			if (countRequest === controller) { countRequest = null; countLoading = false; }
		}
	}
	$effect(() => {
		const target = name;
		untrack(() => { modCount = null; packCount = null; shaderCount = null; void loadCount(target); });
		return () => { countRequest?.abort(); countRequest = null; };
	});
	const launches = getClientLaunchState();
	const operation = $derived(detail.client ? launches.forClient(detail.client.name) : undefined);
	const status = $derived(operation && launchActive(operation) ? launchLabel(operation) : localeState.messages.instance.detail.notRunning);
	const links = $derived(detail.client ? [
		{ title: localeState.messages.instance.detail.mods, hint: modCount !== null ? localeState.messages.instance.count(modCount, 'mods') : countLoading ? localeState.messages.instance.detail.loadingCount : countError ? localeState.messages.instance.detail.countUnavailable : localeState.messages.instance.detail.manageAddMods, icon: Boxes, href: resolve('/clients/[name]/mods', { name: detail.client.name.toLowerCase() }) },
		{ title: localeState.messages.instance.detail.shaders, hint: shaderCount !== null ? localeState.messages.instance.count(shaderCount, 'shaders') : countLoading ? localeState.messages.instance.detail.loadingCount : countError ? localeState.messages.instance.detail.countUnavailable : localeState.messages.instance.detail.manageShaders, icon: Sparkles, href: resolve('/clients/[name]/shader-packs', { name: detail.client.name.toLowerCase() }) },
		{ title: localeState.messages.instance.detail.resourcePacks, hint: packCount !== null ? localeState.messages.instance.count(packCount, 'packs') : countLoading ? localeState.messages.instance.detail.loadingCount : countError ? localeState.messages.instance.detail.countUnavailable : localeState.messages.instance.detail.managePacks, icon: Image, href: resolve('/clients/[name]/resource-packs', { name: detail.client.name.toLowerCase() }) },
	] : []);
	let copied = $state<string | null>(null);
	let copyErrorValue = $state<string | (() => string | null) | null>(null);
	const copyError = $derived(typeof copyErrorValue === 'function' ? copyErrorValue() : copyErrorValue);
	let copying = $state(false);
	const fields = $derived(detail.client ? [
		{ label: localeState.messages.instance.detail.versionType, value: detail.client.versionType === 'release' ? localeState.messages.instance.detail.release : detail.client.versionType === 'snapshot' ? localeState.messages.instance.detail.snapshot : detail.client.versionType, copy: false },
		{ label: localeState.messages.instance.detail.javaRequirement, value: `Java ${detail.client.javaMajor}`, copy: false },
		{ label: localeState.messages.instance.detail.installTarget, value: `${detail.client.os} · ${detail.client.architecture}`, copy: false },
		{ label: localeState.messages.instance.detail.recordFile, value: localeState.messages.instance.count(detail.client.fileCount, 'items'), copy: false },
	] : []);
	async function copy(label: string, value: string) {
		if (copying) return;
		copying = true; copied = null; copyErrorValue = null;
		try { await navigator.clipboard.writeText(value); copied = label; }
		catch { copyErrorValue = () => (localeState.messages.instance.detail.copyUnavailable); }
		finally { copying = false; }
	}
</script>

<svelte:head><title>{localeState.messages.common.instanceTitle(detail.client?.name ?? localeState.messages.nav.instance, localeState.messages.instance.overview)}</title></svelte:head>
<section aria-labelledby="client-overview-title" class="@container min-h-0 flex-1 overflow-y-auto overscroll-contain px-5 py-5 lg:px-6">
	{#if detail.client}
		<div class="relative isolate flex items-center overflow-hidden rounded-xl bg-primary/5 p-4">
			<div class="relative z-10 flex min-w-0 flex-1 items-start gap-3">
				<ClientAvatarEditor id={detail.client.id} name={detail.client.name} class="size-16 shrink-0" />
				<div class="min-w-0 flex-1">
					<p class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground"><span class={`size-1.5 rounded-full ${operation && launchActive(operation) ? 'bg-primary' : 'bg-muted-foreground/50'}`}></span>{status}</p>
					<h1 id="client-overview-title" class="max-w-full break-all text-lg font-semibold tracking-tight">{detail.client.name}</h1>
					<div class="mt-1.5 flex flex-wrap gap-1.5 text-xs text-muted-foreground"><span class="rounded-md bg-card/70 px-2 py-1">Minecraft {detail.client.versionId}</span><span class="rounded-md bg-card/70 px-2 py-1">{detail.client.loader ? `Fabric ${detail.client.loader.version}` : localeState.messages.instance.detail.vanilla}</span></div>
					<div class="mt-3"><ClientLaunchControls client={detail.client}>{#snippet trailing()}<ClientModpackExport client={detail.client!} />{/snippet}</ClientLaunchControls></div>
				</div>
			</div>
			<div class="ml-4 w-[38%] max-w-80 shrink-0"><ClientScreenPreview /></div>
		</div>
		<div class="mt-5 grid grid-cols-1 gap-2 @[32rem]:grid-cols-3">
			{#each links as item (item.title)}
				<a href={item.href} title={(item.title === localeState.messages.instance.detail.mods || item.title === localeState.messages.instance.detail.resourcePacks || item.title === localeState.messages.instance.detail.shaders) && countError ? localeState.messages.instance.detail.countUnavailableHint : undefined} class="group flex min-w-0 items-start gap-2.5 rounded-xl bg-card p-2.5 transition-colors hover:bg-card-hover focus-visible:outline-2 focus-visible:outline-ring">
					<span class="grid size-8 shrink-0 place-items-center rounded-lg bg-primary/8 text-primary"><item.icon size={17} strokeWidth={1.5} aria-hidden="true" /></span>
					<div class="min-w-0 flex-1">
						<div class="flex items-start justify-between gap-2">
							<h2 class="truncate text-body font-medium">{item.title}</h2>
							<ArrowUpRight size={13} class="mt-0.5 shrink-0 text-muted-foreground/50 transition-transform motion-safe:group-hover:-translate-y-0.5 motion-safe:group-hover:translate-x-0.5" aria-hidden="true" />
						</div>
						<p class="mt-1 truncate text-xs leading-4 text-muted-foreground" title={item.hint}>{item.hint}</p>
					</div>
				</a>
			{/each}
		</div>
		<details class="group mt-5 border-t border-border pt-3">
			<summary class="flex cursor-pointer list-none items-center justify-between rounded-md py-1 text-xs text-muted-foreground focus-visible:outline-2 focus-visible:outline-ring [&::-webkit-details-marker]:hidden">{localeState.messages.instance.detail.installationInfo}<ChevronDown size={14} class="transition-transform group-open:rotate-180 motion-reduce:transition-none" aria-hidden="true" /></summary>
			<dl class="mt-2 grid grid-cols-2 gap-x-6 gap-y-3 rounded-lg bg-card p-3">
				{#each fields as field (field.label)}<div class="min-w-0"><dt class="text-xs text-muted-foreground">{field.label}</dt><dd class="mt-1 select-text break-all text-xs">{field.value}</dd></div>{/each}
			</dl>
			<div class="mt-2 flex gap-3">{#each [{ label: localeState.messages.instance.detail.name, value: detail.client.name }, { label: localeState.messages.instance.detail.gameVersion, value: detail.client.versionId }] as item}<button type="button" disabled={copying} onclick={() => void copy(item.label, item.value)} class="inline-flex items-center gap-1 rounded px-1 py-1 text-xs text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring">{#if copied === item.label}<Check size={12} aria-hidden="true" />{:else}<Copy size={12} aria-hidden="true" />{/if}{localeState.messages.instance.copyLabel(item.label)}</button>{/each}</div>
		</details>
	{/if}
	{#if copied}<p role="status" class="mt-2 text-xs text-muted-foreground">{localeState.messages.instance.detail.copied}{copied}</p>{/if}
	{#if copyError}<p role="alert" class="mt-2 text-xs text-destructive">{copyError}</p>{/if}
</section>
