<script lang="ts">
	import { Dialog } from 'bits-ui';
	import { LoaderCircle, Minus, Plus, RotateCcw, X } from '@lucide/svelte';
	import { localeState } from '#lib/locale.svelte.ts';
	import { formatBytes } from '#lib/i18n/index.ts';
	import {
		IMAGE_CROP_OUTPUT_SIZE,
		cropGeometry,
		panCrop,
		validateImageDimensions,
		validateImageFile,
		zoomCrop,
		type CropState,
		type ImageFileError,
		type ImageSize,
	} from '../../image-crop.ts';

	let {
		file,
		onCancel,
		onConfirm,
	}: {
		file: File;
		onCancel: () => void;
		onConfirm: (blob: Blob) => void | Promise<void>;
	} = $props();

	type ErrorKey = NonNullable<ImageFileError> | 'tooManyPixels' | 'tooLargeDimension' | 'decodeFailed' | 'cropFailed';
	const messages = $derived(localeState.messages.common.imageCrop);
	const fileError = $derived(validateImageFile(file));
	const fileLabel = $derived(`${file.name} · ${formatBytes(file.size, localeState.current)}`);

	let imageElement = $state<HTMLImageElement>();
	let objectUrl = $state<string | null>(null);
	let imageSize = $state<ImageSize | null>(null);
	let error = $state<ErrorKey | null>(null);
	let zoom = $state(1);
	let offsetX = $state(0);
	let offsetY = $state(0);
	let viewportWidth = $state(300);
	let dragging = $state(false);
	let dragStart = { x: 0, y: 0, offsetX: 0, offsetY: 0 };
	let confirming = $state(false);
	let activeUrl: string | null = null;
	let generation = 0;

	const frame = $derived(
		imageSize && viewportWidth > 0
			? cropGeometry(imageSize, { width: viewportWidth, height: viewportWidth }, { zoom, offsetX, offsetY })
			: null,
	);
	const ready = $derived(!!imageSize && !error && !fileError);
	const errorMessage = $derived(error ? messages[error] : '');

	$effect(() => {
		const current = ++generation;
		if (activeUrl) {
			URL.revokeObjectURL(activeUrl);
			activeUrl = null;
		}
		imageSize = null;
		objectUrl = null;
		error = fileError;
		zoom = 1;
		offsetX = 0;
		offsetY = 0;
		if (fileError) return;
		const url = URL.createObjectURL(file);
		activeUrl = url;
		objectUrl = url;
		return () => {
			if (generation !== current) return;
			URL.revokeObjectURL(url);
			if (activeUrl === url) activeUrl = null;
		};
	});

	function loaded() {
		if (!imageElement) return;
		const width = imageElement.naturalWidth;
		const height = imageElement?.naturalHeight ?? 0;
		const dimensionError = validateImageDimensions(width, height);
		if (dimensionError) {
			error = dimensionError;
			imageSize = null;
			return;
		}
		imageSize = { width, height };
		error = null;
		zoom = 1;
		offsetX = 0;
		offsetY = 0;
	}

	function failed() {
		error = 'decodeFailed';
		imageSize = null;
	}

	function currentState(): CropState {
		return { zoom, offsetX, offsetY };
	}

	function setZoom(value: number) {
		if (!imageSize) return;
		const next = zoomCrop(imageSize, { width: viewportWidth, height: viewportWidth }, currentState(), value);
		zoom = next.zoom;
		offsetX = next.offsetX;
		offsetY = next.offsetY;
	}

	function reset() {
		zoom = 1;
		offsetX = 0;
		offsetY = 0;
	}

	function pointerDown(event: PointerEvent) {
		if (!ready || event.button !== 0) return;
		const target = event.currentTarget;
		if (!(target instanceof HTMLElement)) return;
		dragging = true;
		dragStart = { x: event.clientX, y: event.clientY, offsetX, offsetY };
		target.setPointerCapture(event.pointerId);
	}

	function pointerMove(event: PointerEvent) {
		if (!dragging || !imageSize) return;
		const next = panCrop(
			imageSize,
			{ width: viewportWidth, height: viewportWidth },
			{ zoom, offsetX: dragStart.offsetX + event.clientX - dragStart.x, offsetY: dragStart.offsetY + event.clientY - dragStart.y },
			0,
			0,
		);
		offsetX = next.offsetX;
		offsetY = next.offsetY;
	}

	function pointerUp(event: PointerEvent) {
		dragging = false;
		const target = event.currentTarget;
		if (target instanceof HTMLElement && target.hasPointerCapture(event.pointerId)) {
			target.releasePointerCapture(event.pointerId);
		}
	}

	function keydown(event: KeyboardEvent) {
		if (!imageSize) return;
		const step = event.shiftKey ? 40 : 12;
		if (event.key === 'ArrowLeft') { event.preventDefault(); panBy(-step, 0); }
		else if (event.key === 'ArrowRight') { event.preventDefault(); panBy(step, 0); }
		else if (event.key === 'ArrowUp') { event.preventDefault(); panBy(0, -step); }
		else if (event.key === 'ArrowDown') { event.preventDefault(); panBy(0, step); }
		else if (event.key === '+' || event.key === '=') { event.preventDefault(); setZoom(zoom + 0.25); }
		else if (event.key === '-') { event.preventDefault(); setZoom(zoom - 0.25); }
		else if (event.key === '0') { event.preventDefault(); reset(); }
	}

	function panBy(deltaX: number, deltaY: number) {
		if (!imageSize) return;
		const next = panCrop(imageSize, { width: viewportWidth, height: viewportWidth }, currentState(), deltaX, deltaY);
		offsetX = next.offsetX;
		offsetY = next.offsetY;
	}

	function wheel(event: WheelEvent) {
		if (!ready) return;
		event.preventDefault();
		setZoom(zoom - event.deltaY * 0.002);
	}

	async function confirm() {
		if (!ready || !imageSize || !imageElement || confirming) return;
		confirming = true;
		error = null;
		try {
			const geometry = cropGeometry(
				imageSize,
				{ width: viewportWidth, height: viewportWidth },
				currentState(),
			);
			const canvas = document.createElement('canvas');
			canvas.width = IMAGE_CROP_OUTPUT_SIZE;
			canvas.height = IMAGE_CROP_OUTPUT_SIZE;
			const context = canvas.getContext('2d');
			if (!context) throw new Error('canvas unavailable');
			context.drawImage(
				imageElement,
				geometry.sourceX,
				geometry.sourceY,
				geometry.sourceSize,
				geometry.sourceSize,
				0,
				0,
				IMAGE_CROP_OUTPUT_SIZE,
				IMAGE_CROP_OUTPUT_SIZE,
			);
			const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'));
			if (!blob) throw new Error('png encoding failed');
			await onConfirm(blob);
		} catch {
			error = 'cropFailed';
		} finally {
			confirming = false;
		}
	}
</script>

<Dialog.Root open={true} onOpenChange={(open) => { if (!open) onCancel(); }}>
	<Dialog.Portal to=".app-window">
		<Dialog.Overlay class="fixed inset-0 z-40 bg-black/25" />
		<Dialog.Content class="fixed top-1/2 left-1/2 z-50 w-[min(420px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 rounded-xl bg-popover p-4 text-popover-foreground shadow-xl outline-none">
			<div class="overflow-y-auto" style="max-height:calc(100dvh - 64px)">
			<div class="flex items-start justify-between gap-3">
				<div>
					<Dialog.Title class="text-sm font-semibold">{messages.title}</Dialog.Title>
					<Dialog.Description class="mt-1 text-[12px] leading-4 text-muted-foreground">{messages.description}</Dialog.Description>
				</div>
				<button type="button" aria-label={localeState.messages.common.close} title={localeState.messages.common.close} onclick={onCancel} class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring"><X size={14} aria-hidden="true" /></button>
			</div>

			<div class="mt-3 space-y-3">
				<div class="relative mx-auto aspect-square w-full max-w-[320px] overflow-hidden rounded-xl bg-chrome-hover">
					{#if objectUrl && !fileError}
						<img
							bind:this={imageElement}
							src={objectUrl}
							alt=""
							draggable="false"
							onload={loaded}
							onerror={failed}
							class={`absolute top-1/2 left-1/2 max-w-none select-none ${dragging ? 'cursor-grabbing' : 'cursor-grab'} ${ready ? '' : 'opacity-30'}`}
							style={frame ? `width:${frame.displayWidth}px;height:${frame.displayHeight}px;transform:translate(-50%,-50%) translate(${frame.offsetX}px,${frame.offsetY}px)` : 'width:100%;height:100%;object-fit:cover'}
						/>
					{/if}
					{#if !ready && !errorMessage}
						<div class="absolute inset-0 grid place-items-center text-muted-foreground"><LoaderCircle size={20} class="animate-spin motion-reduce:animate-none" aria-hidden="true" /><span class="sr-only">{messages.loadingImage}</span></div>
					{/if}
					<button
						type="button"
						aria-label={messages.preview}
						class="absolute inset-0 touch-none appearance-none border-0 bg-transparent p-0 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-ring"
						onpointerdown={pointerDown}
						onpointermove={pointerMove}
						onpointerup={pointerUp}
						onpointercancel={pointerUp}
						onkeydown={keydown}
						onwheel={wheel}
					></button>
				</div>

				<div class="flex items-center gap-2">
					<button type="button" aria-label={localeState.messages.common.imageCrop.zoomOut} title={localeState.messages.common.imageCrop.zoomOut} disabled={!ready || zoom <= 1} onclick={() => setZoom(zoom - 0.25)} class="grid size-8 shrink-0 place-items-center rounded-lg text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Minus size={14} aria-hidden="true" /></button>
					<input type="range" min="1" max="4" step="0.01" value={zoom} disabled={!ready} aria-label={messages.zoom} oninput={(event) => setZoom(Number(event.currentTarget.value))} class="min-w-0 flex-1 accent-primary disabled:opacity-40" />
					<button type="button" aria-label={localeState.messages.common.imageCrop.zoomIn} title={localeState.messages.common.imageCrop.zoomIn} disabled={!ready || zoom >= 4} onclick={() => setZoom(zoom + 0.25)} class="grid size-8 shrink-0 place-items-center rounded-lg text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><Plus size={14} aria-hidden="true" /></button>
					<button type="button" aria-label={messages.reset} title={messages.reset} disabled={!ready} onclick={reset} class="grid size-8 shrink-0 place-items-center rounded-lg text-muted-foreground hover:bg-chrome-hover focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40"><RotateCcw size={14} aria-hidden="true" /></button>
				</div>

				<div class="flex flex-wrap items-center justify-between gap-1 text-[12px] text-muted-foreground">
					<span class="min-w-0 truncate" title={fileLabel}>{fileLabel}</span>
					<span>{messages.hint}</span>
				</div>
				{#if errorMessage}<p role="alert" class="text-xs text-destructive">{errorMessage}</p>{/if}

				<div class="flex items-center justify-end gap-2 pt-1">
					<button type="button" onclick={onCancel} class="inline-flex h-8 items-center rounded-lg px-3 text-xs text-muted-foreground hover:bg-chrome-hover">{localeState.messages.common.cancel}</button>
					<button type="button" disabled={!ready || confirming} onclick={confirm} class="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40">
						{#if confirming}<LoaderCircle size={13} class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else}{messages.apply}{/if}
					</button>
				</div>
			</div>
			</div>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>
