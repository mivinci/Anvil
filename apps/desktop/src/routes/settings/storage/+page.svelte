<script lang="ts">
	import { Dialog } from 'bits-ui';
	import { onDestroy } from 'svelte';
	import { formatBytes } from '#lib/download-progress.ts';
	import { localeState } from '#lib/locale.svelte.ts';
	import { confirmResourceCleanup, previewResourceCleanup, type ResourceCleanupPreview } from '#lib/runtime.ts';
	import { runtimeErrorCode } from '#lib/runtime-errors.ts';

	const text = $derived(localeState.messages.settings);
	const copy = $derived(text.resourceCleanup);
	const secondary = 'rounded-lg bg-secondary px-3 py-1.5 text-xs font-medium focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40';
	let open = $state(false);
	let preview = $state<ResourceCleanupPreview | null>(null);
	let checking = $state(false);
	let cleaning = $state(false);
	let freed = $state<number | null>(null);
	let error = $state<'previewFailed' | 'cleanupFailed' | 'busy' | 'changed' | null>(null);
	let controller: AbortController | undefined;

	function close() {
		if (cleaning) return;
		open = false;
		controller?.abort();
	}

	async function refresh() {
		controller?.abort();
		const request = new AbortController();
		controller = request;
		checking = true;
		preview = null;
		error = null;
		try {
			const result = await previewResourceCleanup(request.signal);
			if (!request.signal.aborted) preview = result;
		} catch (failure) {
			if (!request.signal.aborted) {
				error = runtimeErrorCode(failure) === 'busy' ? 'busy' : 'previewFailed';
			}
		} finally {
			if (controller === request) checking = false;
		}
	}

	async function confirm() {
		if (cleaning || !preview?.objects) return;
		cleaning = true;
		const revision = preview.revision;
		try {
			const result = await confirmResourceCleanup(revision);
			freed = result.bytes;
			open = false;
		} catch (failure) {
			const code = runtimeErrorCode(failure);
			error = code === 'busy' ? 'busy' : code === 'conflict' ? 'changed' : 'cleanupFailed';
			preview = null;
		} finally {
			cleaning = false;
		}
	}

	onDestroy(() => controller?.abort());
</script>

<section class="space-y-2" aria-labelledby="resource-cleanup-title">
	<Dialog.Root {open} onOpenChange={next => {
		if (next) {
			open = true;
			freed = null;
			void refresh();
		} else close();
	}}>
		<div class="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-card px-3.5 py-3">
			<div class="min-w-0 flex-1">
				<h2 id="resource-cleanup-title" class="text-body font-medium">{copy.title}</h2>
				<p class="mt-1 text-xs leading-5 text-muted-foreground">{copy.description}</p>
			</div>
			<Dialog.Trigger class={secondary}>{copy.preview}</Dialog.Trigger>
		</div>
		<Dialog.Portal to=".app-window">
			<Dialog.Overlay class="fixed inset-0 z-40 bg-black/25" />
			<Dialog.Content class="fixed top-1/2 left-1/2 z-50 w-[min(360px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 rounded-xl bg-popover p-4 text-popover-foreground shadow-lg outline-none"
				onEscapeKeydown={event => { if (cleaning) event.preventDefault(); }}
				onInteractOutside={event => { if (cleaning) event.preventDefault(); }}>
				<Dialog.Title class="text-sm font-semibold">{copy.title}</Dialog.Title>
				<Dialog.Description class="mt-2 text-xs leading-5 text-muted-foreground">{copy.description}</Dialog.Description>
				{#if error}
					<p role="alert" class="mt-2 text-xs leading-5 text-destructive">{copy[error]}</p>
				{:else}
					<p role="status" class="mt-2 text-xs leading-5">
						{#if checking}{copy.checking}
						{:else if preview?.objects}{copy.estimate(formatBytes(preview.bytes, localeState.current), new Intl.NumberFormat(localeState.current).format(preview.objects))}
						{:else}{copy.empty}{/if}
					</p>
				{/if}
				<div class="mt-4 flex flex-wrap justify-end gap-2">
					<Dialog.Close class={secondary} disabled={cleaning}>{localeState.messages.common.cancel}</Dialog.Close>
					{#if error}
						<button type="button" class={secondary} onclick={() => void refresh()}>{copy.refresh}</button>
					{:else if preview?.objects}
						<button type="button" class="rounded-lg bg-destructive px-3 py-1.5 text-xs font-medium text-white focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40" disabled={cleaning} onclick={() => void confirm()}>{cleaning ? copy.cleaning : copy.confirm}</button>
					{/if}
				</div>
			</Dialog.Content>
		</Dialog.Portal>
	</Dialog.Root>
	{#if freed !== null}<p role="status" class="px-1 text-xs leading-5 text-muted-foreground">{copy.complete(formatBytes(freed, localeState.current))}</p>{/if}
</section>
