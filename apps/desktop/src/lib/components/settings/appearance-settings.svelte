<script lang="ts">
	import { Monitor, Moon, Sun } from '@lucide/svelte';
	import { getAppearance, type ColorMode } from '../../appearance';
	import ColorControl from './color-control.svelte';
	const appearance = getAppearance();
	let editingMode = $state<ColorMode>('light');
	let editing = $state(false);
	const colorMode = $derived(editing ? editingMode : appearance.resolvedMode);
	const swatches = ['#827099', '#b5a0ce', '#688c83', '#7393b3', '#b88472'];
	const modes = [
		{ value: 'system', label: '跟随系统', icon: Monitor },
		{ value: 'light', label: '浅色', icon: Sun },
		{ value: 'dark', label: '深色', icon: Moon },
	] as const;
</script>

<div class="space-y-5">
	<section aria-labelledby="visual-style" class="space-y-2">
		<h2 id="visual-style" class="text-[13px] font-medium">视觉风格</h2>
		<div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-3 rounded-lg border border-border bg-card px-3.5 py-3">
			<div class="flex items-center gap-3">
				<div><h3 class="text-[13px] font-medium">显示模式</h3></div>
			</div>
			<div class="grid shrink-0 grid-cols-3 gap-2" role="group" aria-label="显示模式">
				{#each modes as mode}
					{@const Icon = mode.icon}
					<button type="button" aria-pressed={appearance.mode === mode.value} onclick={() => appearance.setMode(mode.value)} class={`group w-[84px] rounded-md p-1 text-left transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring ${appearance.mode === mode.value ? 'bg-primary/8 ring-2 ring-primary' : 'hover:bg-chrome-hover'}`}>
						<div aria-hidden="true" class={`relative isolate flex h-12 overflow-hidden rounded-md ${mode.value === 'dark' ? 'bg-[#303133]' : mode.value === 'light' ? 'bg-[#e7e7e5]' : 'bg-[#e7e7e5]'}`}>
							{#if mode.value === 'system'}<span class="pointer-events-none absolute inset-y-0 right-0 -z-10 w-1/2 bg-[#303133]"></span>{/if}
							<div class="flex w-1/4 flex-col items-center gap-1.5 pt-2"><span class="size-1.5 rounded-sm bg-neutral-500/40"></span><span class="size-1.5 rounded-sm bg-primary/60"></span><span class="size-1.5 rounded-sm bg-neutral-500/40"></span></div>
							<!-- The sidebar occupies 1/4 of the outer width; include the right margin when mapping its midpoint into the content pane. -->
							<div class={`relative isolate my-1.5 mr-1.5 flex flex-1 flex-col gap-1.5 overflow-hidden rounded-sm p-1.5 ${mode.value === 'dark' ? 'bg-[#191a1b]' : mode.value === 'light' ? 'bg-white' : 'bg-white'}`}>{#if mode.value === 'system'}<span class="pointer-events-none absolute inset-y-0 right-0 left-[calc((100%_+_0.375rem)/3)] -z-10 bg-[#191a1b]"></span>{/if}<span class="h-0.5 w-2/3 rounded-full bg-neutral-400/40"></span><span class="h-0.5 w-1/2 rounded-full bg-neutral-400/25"></span><span class="mt-auto h-1.5 w-1/3 self-end rounded-sm bg-primary"></span></div>
						</div>
						<span class="mt-1.5 flex items-center justify-center gap-1 pb-0.5 text-xs"><Icon size={12} aria-hidden="true" />{mode.label}</span>
					</button>
				{/each}
			</div>
		</div>
	</section>
	<section aria-labelledby="theme-colors" class="space-y-2">
		<h2 id="theme-colors" class="text-[13px] font-medium">主题配色</h2>
		<div class="divide-y divide-border rounded-lg border border-border bg-card px-3.5">
			<div class="flex min-h-10 items-center justify-between gap-4 py-2.5">
				<label for="theme-preset" class="text-[13px]">主题</label>
				<select id="theme-preset" value={appearance.colors.theme} onchange={(event) => appearance.setTheme(event.currentTarget.value as 'default' | 'custom')} class="h-7 rounded-md border border-border bg-main-pane px-2 text-xs text-foreground focus-visible:outline-2 focus-visible:outline-ring"><option value="default">水墨淡紫</option><option value="custom">自定义</option></select>
			</div>
			<div class="flex min-h-10 flex-wrap items-center justify-between gap-2 py-2.5">
				<span class="text-[13px]">编辑配色</span>
				<div class="inline-flex rounded-[10px] bg-chrome-hover p-[3px]" role="group" aria-label="编辑浅色或深色配色">
					{#each ['light', 'dark'] as mode}<button type="button" aria-pressed={colorMode === mode} onclick={() => { editingMode = mode as ColorMode; editing = true; }} class={`rounded-[7px] px-2 py-0.5 text-xs leading-4 focus-visible:outline-2 focus-visible:outline-ring ${colorMode === mode ? 'bg-chrome-selected text-accent-foreground' : 'text-muted-foreground'}`}>{mode === 'light' ? '浅色' : '深色'}</button>{/each}
				</div>
			</div>
			{#if appearance.colors.theme === 'custom'}
				{#each [{ key: 'background', label: '内容背景' }, { key: 'foreground', label: '文字颜色' }] as field}
					<div class="flex min-h-10 items-center justify-between gap-4 py-2.5"><span class="text-[13px]">{field.label}</span><ColorControl label={field.label} value={appearance.colors[colorMode][field.key as 'background' | 'foreground']} onChange={(value) => appearance.setColor(colorMode, field.key as 'background' | 'foreground', value)} /></div>
				{/each}
			{/if}
			<div class="flex min-h-10 flex-wrap items-center justify-between gap-3 py-2.5">
				<span class="text-[13px]">强调色</span>
				<div class="flex flex-wrap items-center justify-end gap-2">
					<div class="flex gap-1.5" role="group" aria-label="强调色色板">{#each swatches as color}<button type="button" aria-label={`强调色 ${color}`} aria-pressed={appearance.colors[colorMode].accent === color} onclick={() => appearance.setColor(colorMode, 'accent', color)} class={`size-3.5 rounded-full border border-border focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring ${appearance.colors[colorMode].accent === color ? 'ring-1 ring-ring ring-offset-2 ring-offset-card' : ''}`} style:background-color={color}></button>{/each}</div>
					<ColorControl label="强调色" value={appearance.colors[colorMode].accent} onChange={(value) => appearance.setColor(colorMode, 'accent', value)} />
				</div>
			</div>
			<div class="flex min-h-10 items-center justify-between gap-4 py-2.5"><div><span class="text-[13px]">窗口材质</span></div><span class="shrink-0 text-xs text-muted-foreground">原生毛玻璃</span></div>
		</div>
		<div class="flex flex-wrap items-center justify-between gap-3 pt-2"><p class="text-[11px] text-muted-foreground">浅色与深色配色分别保存，切换对应模式可预览。</p><button type="button" onclick={() => appearance.resetColors()} class="shrink-0 rounded px-1.5 py-1 text-xs text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring">恢复默认配色</button></div>
		{#if appearance.saveError}<p role="alert" class="text-[11px] text-destructive">{appearance.saveError}</p>{/if}
	</section>
</div>
