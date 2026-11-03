<script lang="ts">
	import { Select } from 'bits-ui';
	import { Check, ChevronDown } from '@lucide/svelte';
	import { localeNames, locales } from '#lib/i18n/index.ts';
	import { localeState, type LocalePreference } from '#lib/locale.svelte.ts';
	let { id, labelledby, describedby }: { id: string; labelledby: string; describedby?: string } = $props();
</script>

		<Select.Root type="single" value={localeState.preference} onValueChange={(value) => { if (value) localeState.setLocale(value as LocalePreference); }}>
			<Select.Trigger {id} aria-labelledby={labelledby} aria-describedby={describedby} class="flex h-8 max-w-full shrink-0 items-center justify-between gap-3 rounded-lg border border-border bg-main-pane px-2.5 text-xs outline-none hover:bg-chrome-hover focus-visible:ring-2 focus-visible:ring-ring">
				<span lang={localeState.preference === 'system' ? localeState.current : localeState.preference}>{localeState.preference === 'system' ? localeState.messages.settings.systemLanguage : localeNames[localeState.preference]}</span>
				<ChevronDown size={13} class="shrink-0 text-muted-foreground" aria-hidden="true" />
			</Select.Trigger>
			<Select.Portal>
				<Select.Content sideOffset={5} align="end" class="z-50 min-w-40 rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-lg outline-none">
					{#each ['system', ...locales] as value}
						{@const preference = value as LocalePreference}
						{@const label = preference === 'system' ? localeState.messages.settings.systemLanguage : localeNames[preference]}
						<Select.Item value={preference} {label} class="flex cursor-default items-center justify-between gap-4 rounded-md px-2 py-1.5 text-xs outline-none data-highlighted:bg-chrome-hover">
							<span lang={preference === 'system' ? localeState.current : preference}>{label}</span>
							{#if localeState.preference === preference}<Check size={13} class="text-primary" aria-hidden="true" />{/if}
						</Select.Item>
					{/each}
				</Select.Content>
			</Select.Portal>
		</Select.Root>
