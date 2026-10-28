<script lang="ts">
	import { Dialog } from 'bits-ui';
	import { ArrowUpFromLine, LoaderCircle, X } from '@lucide/svelte';
	import type { ClientDetails, ModpackExportSelection } from '../../runtime';
	import { getClientInstallState } from '../../client-install-state.svelte.js';
	import { getClientLaunchState, launchActive } from '../../client-launch-state.svelte.js';
	import { getModpackExportState } from '../../modpack-export-state.svelte.js';
	import { getResourceInstallState } from '../../resource-install-state.svelte.js';
	let { client }: { client: ClientDetails } = $props();
	const exports = getModpackExportState();
	const launches = getClientLaunchState();
	const installs = getClientInstallState();
	const resources = getResourceInstallState();
	let open = $state(false);
	let name = $state('');
	let version = $state('1.0.0');
	let selection = $state<ModpackExportSelection>({ mods: true, resourcePacks: true, shaderPacks: true, config: true, worlds: false });
	const operation = $derived(launches.forClient(client.name));
	const busy = $derived(exports.active || exports.starting || launches.pending.includes(client.name) || launchActive(operation) || installs.active || installs.starting || resources.active || resources.starting);
	const selected = $derived(Object.values(selection).some(Boolean));
	const labels: { key: keyof ModpackExportSelection; label: string }[] = [
		{ key: 'mods', label: '模组' },
		{ key: 'resourcePacks', label: '资源包' },
		{ key: 'shaderPacks', label: '光影' },
		{ key: 'config', label: '配置' },
		{ key: 'worlds', label: '存档' },
	];
	function show() {
		if (busy) return;
		name = client.name;
		version = client.modpackVersionId || '1.0.0';
		selection = { mods: true, resourcePacks: true, shaderPacks: true, config: true, worlds: false };
		exports.error = null;
		open = true;
	}
	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!selected || !name.trim() || !version.trim() || exports.starting) return;
		const ok = await exports.start(client.name, { name: name.trim(), versionId: version.trim(), selection: { ...selection } });
		if (ok) open = false;
	}
</script>

<button type="button" disabled={busy} onclick={show} title={busy ? '任务进行中，暂时不能导出' : '导出自包含整合包'} class="inline-flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><ArrowUpFromLine size={13} aria-hidden="true" />{exports.active ? '导出中…' : '导出'}</button>

<Dialog.Root bind:open>
	<Dialog.Portal to=".app-window">
		<Dialog.Overlay class="fixed inset-0 z-40 bg-black/25" />
		<Dialog.Content class="fixed top-1/2 left-1/2 z-50 w-[min(420px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 rounded-xl bg-popover p-4 text-popover-foreground shadow-xl outline-none">
			<div class="flex items-start justify-between gap-3">
				<div><Dialog.Title class="text-sm font-semibold">导出整合包</Dialog.Title><Dialog.Description class="mt-1 text-[11px] leading-4 text-muted-foreground">导出自包含的 .mrpack，保存到系统下载目录；关闭面板后任务继续。</Dialog.Description></div>
				<button type="button" aria-label="关闭" title="关闭" onclick={() => { open = false; }} class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring"><X size={14} aria-hidden="true" /></button>
			</div>
			<form class="mt-3 space-y-3" onsubmit={submit}>
				<label class="block text-[11px] text-muted-foreground">整合包名称<input bind:value={name} maxlength="256" required class="mt-1 block h-9 w-full rounded-lg bg-chrome-hover px-3 text-xs text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring" /></label>
				<label class="block text-[11px] text-muted-foreground">包版本<input bind:value={version} maxlength="128" required class="mt-1 block h-9 w-full rounded-lg bg-chrome-hover px-3 text-xs text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring" /></label>
				<fieldset><legend class="text-[11px] text-muted-foreground">包含内容</legend><div class="mt-1.5 grid grid-cols-2 gap-1.5">{#each labels as item (item.key)}<label class="flex items-center gap-2 rounded-lg bg-card px-2.5 py-2 text-xs"><input type="checkbox" bind:checked={selection[item.key]} class="size-4 shrink-0 accent-primary" />{item.label}</label>{/each}</div></fieldset>
				{#if exports.error}<p role="alert" class="break-words text-xs text-destructive">{exports.error}</p>{/if}
				<div class="flex items-center justify-end gap-2 pt-1"><button type="button" class="inline-flex h-8 items-center rounded-lg px-3 text-xs text-muted-foreground hover:bg-chrome-hover" onclick={() => { open = false; }}>取消</button><button type="submit" disabled={!selected || exports.starting} class="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">{#if exports.starting}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />创建中…{:else}<ArrowUpFromLine size={13} aria-hidden="true" />开始导出{/if}</button></div>
			</form>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
