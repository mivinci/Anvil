<script lang="ts">
	import { untrack } from 'svelte';
	import { RefreshCw } from '@lucide/svelte';
	import ClientDelete from '#lib/components/clients/client-delete.svelte';
	import { getClientDetailState } from '#lib/client-detail-state.svelte.js';
	import { getClientLaunchSettings, saveClientLaunchSettings, getDeviceMemory, RuntimeRequestError, type ClientLaunchSettings } from '#lib/runtime.js';
	const detail = getClientDetailState();
	let saved = $state<ClientLaunchSettings | null>(null);
	let maximum = $state<number | undefined>(2);
	let initial = $state<number | undefined>(0.5);
	let automaticInitial = $state(true);
	let customSize = $state(false);
	let width = $state<number | undefined>(1280);
	let height = $state<number | undefined>(720);
	let memory = $state<number | null>(null);
	let loading = $state(false);
	let saving = $state(false);
	let uncertain = $state(false);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let readController: AbortController | null = null;
	let saveController: AbortController | null = null;
	let currentName = '';
	const mib = (value: number | undefined) => value === undefined ? NaN : value * 1024;
	const draft = $derived<ClientLaunchSettings>({ schemaVersion: 1, revision: saved?.revision ?? 0, maxMemoryMib: mib(maximum), initialMemoryMib: automaticInitial ? null : mib(initial), resolution: customSize ? { width: width ?? NaN, height: height ?? NaN } : null });
	const dirty = $derived(!!saved && JSON.stringify(saved) !== JSON.stringify(draft));
	const validation = $derived(!Number.isInteger(draft.maxMemoryMib) || draft.maxMemoryMib < 512 || draft.maxMemoryMib > 131072 ? '最大内存须为 0.5–128 GiB，并能换算为整数 MiB。' : draft.initialMemoryMib !== null && (!Number.isInteger(draft.initialMemoryMib) || draft.initialMemoryMib < 128 || draft.initialMemoryMib > draft.maxMemoryMib) ? '初始内存须至少 0.125 GiB，且不能大于最大内存。' : draft.resolution && (!Number.isInteger(draft.resolution.width) || draft.resolution.width < 320 || draft.resolution.width > 16384 || !Number.isInteger(draft.resolution.height) || draft.resolution.height < 240 || draft.resolution.height > 16384) ? '宽度须为 320–16384 px，高度须为 240–16384 px。' : null);
	const disabled = $derived(loading || saving || !saved);
	function populate(value: ClientLaunchSettings) {
		maximum = value.maxMemoryMib / 1024; automaticInitial = value.initialMemoryMib === null; initial = (value.initialMemoryMib ?? 512) / 1024;
		customSize = value.resolution !== null; width = value.resolution?.width ?? 1280; height = value.resolution?.height ?? 720;
	}
	async function load(name: string) {
		readController?.abort();
		const controller = new AbortController(); readController = controller;
		loading = true; error = null; notice = null;
		try {
			const value = await getClientLaunchSettings(name, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (controller.signal.aborted) return;
			saved = value; populate(value); uncertain = false;
		} catch (cause) { if (!controller.signal.aborted) { uncertain = true; error = cause instanceof RuntimeRequestError ? cause.message : '无法读取实例设置，请重试。'; } }
		finally { if (readController === controller) { readController = null; loading = false; } }
	}
	$effect(() => {
		const name = detail.client?.name;
		const controller = new AbortController();
		if (name) untrack(() => {
			if (name !== currentName) { currentName = name; saved = null; uncertain = false; saving = false; notice = null; }
			void load(name);
			void getDeviceMemory(controller.signal).then(value => { if (!controller.signal.aborted) memory = value.totalMib; }).catch(() => {});
		});
		return () => { controller.abort(); readController?.abort(); saveController?.abort(); readController = null; saveController = null; };
	});
	async function save() {
		if (disabled || uncertain || !dirty || validation || !detail.client) return;
		const name = detail.client.name;
		const controller = new AbortController(); saveController = controller;
		saving = true; error = null; notice = null;
		try {
			const value = await saveClientLaunchSettings(name, draft, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (controller.signal.aborted) return;
			saved = value; populate(value); notice = '已保存，下次启动生效。';
		} catch (cause) {
			if (!controller.signal.aborted) {
				uncertain = !(cause instanceof RuntimeRequestError && cause.status === 400);
				error = cause instanceof RuntimeRequestError ? `${cause.message}${uncertain ? ' 请重新读取后再保存。' : ''}` : '保存结果未确认，请重新读取后再修改。';
			}
		} finally { if (saveController === controller) { saveController = null; saving = false; } }
	}
	function reset() { populate({ schemaVersion: 1, revision: saved?.revision ?? 0, maxMemoryMib: 2048, initialMemoryMib: null, resolution: null }); notice = null; }
	const inputClass = 'h-7 w-20 rounded-md border border-border bg-background px-2 text-right text-[12px] outline-none focus-visible:border-ring disabled:opacity-50';
</script>
<svelte:head><title>{detail.client?.name ?? '实例'} · 实例设置 · Anvil</title></svelte:head>
<section aria-labelledby="client-settings-title" class="min-h-0 flex-1 overflow-y-auto overscroll-contain px-5 py-5 lg:px-6">
	<div class="flex items-center justify-between gap-3"><h1 id="client-settings-title" class="text-base font-semibold tracking-tight">实例设置</h1><button type="button" aria-label="重新读取实例设置" title="重新读取（放弃未保存的修改）" disabled={loading || saving || !detail.client} onclick={() => { if (detail.client) void load(detail.client.name); }} class="grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RefreshCw size={14} class={loading ? 'animate-spin motion-reduce:animate-none' : ''} aria-hidden="true" /></button></div>
	<p class="mt-1 text-[11px] leading-5 text-muted-foreground">仅作用于当前实例，保存后下次启动生效。</p>
	{#if error}<p role="alert" class="mt-3 text-[12px] text-destructive">{error}<button type="button" disabled={loading || saving} onclick={() => { if (detail.client) void load(detail.client.name); }} class="ml-2 text-primary disabled:opacity-50">重新读取</button></p>{/if}
	{#if loading && !saved}<p role="status" class="mt-4 text-[13px] text-muted-foreground">正在读取设置…</p>{/if}
	<form onsubmit={(event) => { event.preventDefault(); void save(); }} class="mt-5 space-y-5">
		<fieldset disabled={disabled} class="min-w-0">
			<legend class="mb-2 text-[13px] font-medium">内存</legend>
			<div class="divide-y divide-border rounded-lg border border-border bg-card px-3.5">
				<div class="flex min-h-12 flex-wrap items-center justify-between gap-2 py-2"><label for="maximum-memory" class="text-[13px]">最大内存</label><div class="flex items-center gap-2"><input id="maximum-memory" type="number" min="0.5" max="128" step="any" bind:value={maximum} class={inputClass} /><span class="text-[11px] text-muted-foreground">GiB</span></div></div>
				<div class="flex min-h-10 flex-wrap items-center justify-between gap-2 py-2"><span class="text-[11px] text-muted-foreground">{memory !== null ? `设备内存 ${(memory / 1024).toFixed(1)} GiB` : '设备内存暂不可用'} · 内存上限不等于预占</span><div class="flex gap-1">{#each [1, 2, 4, 8] as value}<button type="button" aria-label={`最大内存 ${value} GiB`} aria-pressed={maximum === value} onclick={() => maximum = value} class={`rounded-md px-2 py-1 text-[11px] ${maximum === value ? 'bg-chrome-selected text-accent-foreground' : 'text-muted-foreground hover:bg-chrome-hover'}`}>{value}G</button>{/each}</div></div>
			</div>
			<details class="mt-2 text-[12px]"><summary class="w-fit cursor-pointer py-1 text-muted-foreground">高级</summary><div class="mt-1 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-card px-3.5 py-2.5"><label for="initial-memory" class="text-[13px]">初始内存</label><div class="flex items-center gap-2"><label class="flex items-center gap-1.5 text-[12px] text-muted-foreground"><input type="checkbox" bind:checked={automaticInitial} class="accent-primary" />自动</label>{#if !automaticInitial}<input id="initial-memory" type="number" min="0.125" max={maximum} step="any" bind:value={initial} class={inputClass} /><span class="text-[11px] text-muted-foreground">GiB</span>{/if}</div></div></details>
		</fieldset>
		<fieldset disabled={disabled} class="min-w-0">
			<legend class="mb-2 text-[13px] font-medium">游戏窗口</legend>
			<div class="divide-y divide-border rounded-lg border border-border bg-card px-3.5">
				<div class="flex min-h-12 flex-wrap items-center justify-between gap-3 py-2"><span class="text-[13px]">启动尺寸</span><div class="flex gap-1 rounded-lg bg-chrome-hover p-1">{#each [{ value: false, label: '默认尺寸' }, { value: true, label: '指定尺寸' }] as option}<button type="button" aria-pressed={customSize === option.value} onclick={() => customSize = option.value} class={`rounded-md px-2.5 py-1 text-[12px] ${customSize === option.value ? 'bg-chrome-selected text-accent-foreground' : 'text-muted-foreground'}`}>{option.label}</button>{/each}</div></div>
				{#if customSize}
					<div class="flex flex-wrap items-center justify-between gap-3 py-2.5"><span class="text-[13px]">宽 × 高</span><div class="flex items-center gap-1.5"><input type="number" aria-label="游戏窗口宽度" min="320" max="16384" step="1" bind:value={width} class={inputClass} /><span class="text-[11px] text-muted-foreground">×</span><input type="number" aria-label="游戏窗口高度" min="240" max="16384" step="1" bind:value={height} class={inputClass} /><span class="text-[11px] text-muted-foreground">px</span></div></div>
					<div class="flex flex-wrap justify-end gap-1 py-2">{#each [[854, 480], [1280, 720], [1920, 1080]] as value}<button type="button" onclick={() => { width = value[0]; height = value[1]; }} class="rounded-md px-2 py-1 text-[11px] text-muted-foreground hover:bg-chrome-hover">{value[0]} × {value[1]}</button>{/each}</div>
				{:else}<p class="py-2 text-[11px] leading-5 text-muted-foreground">沿用游戏自身的窗口设置。</p>{/if}
			</div>
		</fieldset>
		{#if validation && saved}<p role="status" class="text-[12px] text-destructive">{validation}</p>{/if}
		<div class="flex flex-wrap items-center justify-between gap-3 border-t border-border pt-3"><p role="status" class="text-[11px] text-muted-foreground">{saving ? '正在保存…' : dirty ? '有未保存的修改' : notice ?? '当前实例独立保存'}</p><div class="flex items-center gap-2"><button type="button" disabled={disabled || uncertain} onclick={reset} class="h-7 rounded-lg px-2.5 text-[12px] text-muted-foreground hover:bg-chrome-hover disabled:opacity-40">恢复默认</button><button type="submit" disabled={disabled || uncertain || !dirty || !!validation} class="h-7 rounded-lg bg-primary px-3 text-[12px] font-medium text-primary-foreground hover:opacity-90 disabled:opacity-40">保存</button></div></div>
	</form>
	{#if detail.client}<ClientDelete name={detail.client.name} disabled={saving} />{/if}
</section>
