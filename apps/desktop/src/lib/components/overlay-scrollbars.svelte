<script lang="ts">
	import { onMount } from 'svelte';
	type Entry = { element: HTMLElement; expires: number; focused: boolean; changes: MutationObserver };
	type Thumb = { entry: Entry; axis: 'vertical' | 'horizontal'; id: string; x: number; y: number; width: number; height: number; max: number; value: number; travel: number };
	let thumbs = $state<Thumb[]>([]);
	const entries = new Map<HTMLElement, Entry>();
	let frame = 0, timer: ReturnType<typeof setTimeout> | undefined, serial = 0;
	let dragging: { thumb: Thumb; pointer: number; start: number; value: number } | null = null;
	let disposed = false;
	let observer: ResizeObserver;
	function forget(entry: Entry) {
		entry.element.removeAttribute('data-scrollbar-visible');
		observer?.unobserve(entry.element);
		entry.changes.disconnect();
		entries.delete(entry.element);
	}
	function schedule() { if (!frame && !disposed) frame = requestAnimationFrame(update); }
	function reveal(element: HTMLElement) {
		if (disposed) return;
		const style = getComputedStyle(element);
		if (!(/auto|scroll|overlay/.test(style.overflowY) && element.scrollHeight > element.clientHeight + 1) && !(/auto|scroll|overlay/.test(style.overflowX) && element.scrollWidth > element.clientWidth + 1)) return;
		let entry = entries.get(element);
		if (!entry) {
			if (entries.size >= 8) { const oldest = [...entries.values()].find(value => value !== dragging?.thumb.entry && !value.focused); if (oldest) forget(oldest); }
			const changes = new MutationObserver(schedule);
			changes.observe(element, { childList: true, subtree: true, characterData: true });
			entry = { element, expires: 0, focused: false, changes };
			entries.set(element, entry);
			observer?.observe(element);
		}
		entry.expires = performance.now() + 800;
		element.setAttribute('data-scrollbar-visible', '');
		if (!element.id) element.id = `overlay-scroll-${++serial}`;
		schedule();
	}
	function update() {
		frame = 0; clearTimeout(timer);
		const next: Thumb[] = [], now = performance.now();
		const layer = [...document.querySelectorAll<HTMLElement>('[role="dialog"], [role="alertdialog"], [role="menu"]')].filter(element => element.getClientRects().length && element.getAttribute('aria-hidden') !== 'true').at(-1);
		let deadline = Infinity;
		for (const entry of entries.values()) {
			const element = entry.element;
			entry.focused = document.activeElement instanceof HTMLElement && document.activeElement.dataset.overlayScrollbarFor === element.id;
			if (!element.isConnected || (entry.expires <= now && !entry.focused && dragging?.thumb.entry !== entry)) { forget(entry); continue; }
			if (!entry.focused && dragging?.thumb.entry !== entry) deadline = Math.min(deadline, entry.expires);
			if (!element.getClientRects().length) continue;
			if (layer && !layer.contains(element)) continue;
			const rect = element.getBoundingClientRect();
			const clip = { left: 0, top: 0, right: innerWidth, bottom: innerHeight };
			for (let parent = element.parentElement; parent; parent = parent.parentElement) {
				const style = getComputedStyle(parent), bounds = parent.getBoundingClientRect();
				if (/auto|scroll|hidden|clip|overlay/.test(style.overflowX)) { clip.left = Math.max(clip.left, bounds.left + parent.clientLeft); clip.right = Math.min(clip.right, bounds.left + parent.clientLeft + parent.clientWidth); }
				if (/auto|scroll|hidden|clip|overlay/.test(style.overflowY)) { clip.top = Math.max(clip.top, bounds.top + parent.clientTop); clip.bottom = Math.min(clip.bottom, bounds.top + parent.clientTop + parent.clientHeight); }
			}
			const style = getComputedStyle(element);
			for (const axis of ['vertical', 'horizontal'] as const) {
				if (!/auto|scroll|overlay/.test(axis === 'vertical' ? style.overflowY : style.overflowX)) continue;
				const visible = axis === 'vertical' ? element.clientHeight : element.clientWidth;
				const total = axis === 'vertical' ? element.scrollHeight : element.scrollWidth;
				const max = total - visible, track = Math.max(0, visible - 4);
				if (max <= 1 || track <= 0) continue;
				const size = Math.min(track, Math.max(24, track * visible / total));
				const travel = track - size;
				const value = Math.max(0, Math.min(max, axis === 'vertical' ? element.scrollTop : element.scrollLeft));
				const offset = 2 + value / max * travel;
				const x = rect.left + element.clientLeft + (axis === 'vertical' ? element.clientWidth - 6 : offset);
				const y = rect.top + element.clientTop + (axis === 'vertical' ? offset : element.clientHeight - 6);
				const right = Math.min(clip.right, x + (axis === 'vertical' ? 4 : size));
				const bottom = Math.min(clip.bottom, y + (axis === 'vertical' ? size : 4));
				const left = Math.max(clip.left, x), top = Math.max(clip.top, y);
				if (right > left && bottom > top) next.push({ entry, axis, id: element.id, x: left, y: top, width: right - left, height: bottom - top, max, value, travel });
			}
		}
		thumbs = next;
		if (Number.isFinite(deadline)) timer = setTimeout(schedule, Math.max(1, deadline - performance.now()));
	}
	function start(event: PointerEvent, thumb: Thumb) {
		if (event.button !== 0) return;
		event.preventDefault();
		// The thumb belongs to the scroller even when painted outside its DOM
		// subtree. Do not let a popover interpret dragging as an outside click.
		event.stopPropagation();
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		dragging = { thumb, pointer: event.pointerId, start: thumb.axis === 'vertical' ? event.clientY : event.clientX, value: thumb.value };
		schedule();
	}
	function move(event: PointerEvent) {
		if (!dragging || event.pointerId !== dragging.pointer) return;
		const { thumb, start, value } = dragging;
		const next = value + ((thumb.axis === 'vertical' ? event.clientY : event.clientX) - start) * thumb.max / Math.max(1, thumb.travel);
		if (thumb.axis === 'vertical') thumb.entry.element.scrollTop = next; else thumb.entry.element.scrollLeft = next;
		reveal(thumb.entry.element);
	}
	function release(event: PointerEvent) {
		if (!dragging || event.pointerId !== dragging.pointer) return;
		const element = dragging.thumb.entry.element; dragging = null; reveal(element);
	}
	function key(event: KeyboardEvent, thumb: Thumb) {
		const page = thumb.axis === 'vertical' ? thumb.entry.element.clientHeight : thumb.entry.element.clientWidth;
		const delta = { ArrowDown: 40, ArrowRight: 40, ArrowUp: -40, ArrowLeft: -40, PageDown: page, PageUp: -page }[event.key];
		if (delta === undefined && !['Home', 'End'].includes(event.key)) return;
		event.preventDefault();
		const value = event.key === 'Home' ? 0 : event.key === 'End' ? thumb.max : thumb.value + delta!;
		if (thumb.axis === 'vertical') thumb.entry.element.scrollTop = value; else thumb.entry.element.scrollLeft = value;
		reveal(thumb.entry.element);
	}
	onMount(() => {
		observer = new ResizeObserver(schedule);
		const scroll = (event: Event) => { if (event.target instanceof HTMLElement) reveal(event.target); schedule(); };
		const blur = () => { if (dragging) { const element = dragging.thumb.entry.element; dragging = null; reveal(element); } };
		const repaintLayers = () => { if (entries.size) schedule(); };
		const wheel = (event: WheelEvent) => {
			const target = event.target instanceof Element ? event.target.closest<HTMLElement>('[data-overlay-scrollbar-for]') : null;
			if (!target) return;
			const thumb = thumbs.find(value => value.id === target.dataset.overlayScrollbarFor && value.axis === target.getAttribute('aria-orientation'));
			if (!thumb) return;
			event.preventDefault(); event.stopPropagation();
			const scale = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? thumb.entry.element.clientHeight : 1;
			thumb.entry.element.scrollBy({ top: thumb.axis === 'vertical' ? event.deltaY * scale : 0, left: thumb.axis === 'horizontal' ? (event.deltaX || event.deltaY) * scale : event.deltaX * scale });
			reveal(thumb.entry.element);
		};
		document.addEventListener('scroll', scroll, { capture: true, passive: true });
		window.addEventListener('resize', schedule);
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', release);
		window.addEventListener('pointercancel', release);
		window.addEventListener('blur', blur);
		window.addEventListener('pointerdown', repaintLayers, true); window.addEventListener('keydown', repaintLayers, true);
		window.addEventListener('wheel', wheel, { capture: true, passive: false });
		return () => {
			disposed = true; clearTimeout(timer); cancelAnimationFrame(frame); observer.disconnect();
			document.removeEventListener('scroll', scroll, true);
			window.removeEventListener('resize', schedule);
			window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', release); window.removeEventListener('pointercancel', release); window.removeEventListener('blur', blur);
			window.removeEventListener('pointerdown', repaintLayers, true); window.removeEventListener('keydown', repaintLayers, true); window.removeEventListener('wheel', wheel, true);
			for (const entry of entries.values()) forget(entry);
		};
	});
</script>

<div class="pointer-events-none fixed inset-0 z-[60]" data-overlay-scrollbars>
	{#each thumbs as thumb (`${thumb.id}-${thumb.axis}`)}
		<div role="scrollbar" tabindex="0" aria-label={thumb.axis === 'vertical' ? '垂直滚动条' : '水平滚动条'} aria-orientation={thumb.axis} aria-controls={thumb.id} aria-valuemin={0} aria-valuemax={Math.round(thumb.max)} aria-valuenow={Math.round(thumb.value)} data-overlay-scrollbar-for={thumb.id} class="overlay-scrollbar pointer-events-auto absolute touch-none rounded-full bg-muted-foreground/30 hover:bg-muted-foreground/55 focus-visible:bg-muted-foreground/55 focus-visible:outline-1 focus-visible:outline-ring" style:left={`${thumb.x}px`} style:top={`${thumb.y}px`} style:width={`${thumb.width}px`} style:height={`${thumb.height}px`} onpointerdown={event => start(event, thumb)} onlostpointercapture={release} onkeydown={event => key(event, thumb)} onfocus={() => { thumb.entry.focused = true; schedule(); }} onblur={() => { thumb.entry.focused = false; reveal(thumb.entry.element); }}></div>
	{/each}
</div>
