<script lang="ts">
	import { afterNavigate } from '$app/navigation';
	import { ArrowLeft, ArrowRight } from '@lucide/svelte';
	import DownloadTasks from './download-tasks.svelte';

	let historyPosition = $state(0);
	let historyEnd = $state(0);
	let historyPending = $state(false);
	let currentUrl: string | undefined;

	afterNavigate((navigation) => {
		historyPending = false;
		const nextUrl = navigation.to?.url.href;
		if (navigation.type === 'enter') {
			currentUrl = nextUrl;
			return;
		}

		if (navigation.type === 'popstate') {
			historyPosition = Math.max(0, historyPosition + navigation.delta);
		} else if (nextUrl !== currentUrl) {
			historyPosition += 1;
			historyEnd = historyPosition;
		}
		currentUrl = nextUrl;
	});

	function traverseHistory(direction: -1 | 1) {
		if (historyPending || (direction === -1 ? historyPosition === 0 : historyPosition >= historyEnd)) {
			return;
		}
		historyPending = true;
		window.history.go(direction);
	}
</script>

<header
	class="relative z-10 flex h-8 shrink-0 items-center gap-1 bg-transparent pl-21 pr-2 cursor-grab active:cursor-grabbing"
	data-tauri-drag-region
	aria-label="窗口拖动区域"
>
	<button
		type="button"
		class="grid size-6 cursor-default place-items-center rounded-md text-chrome-muted/80 transition-colors enabled:hover:bg-chrome-selected enabled:hover:text-chrome-muted focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:opacity-30"
		aria-label="后退"
		title="后退"
		disabled={historyPending || historyPosition === 0}
		onclick={() => traverseHistory(-1)}
	><ArrowLeft size={16} aria-hidden="true" />
	</button>
	<button
		type="button"
		class="grid size-6 cursor-default place-items-center rounded-md text-chrome-muted/80 transition-colors enabled:hover:bg-chrome-selected enabled:hover:text-chrome-muted focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring disabled:opacity-30"
		aria-label="前进"
		title="前进"
		disabled={historyPending || historyPosition >= historyEnd}
		onclick={() => traverseHistory(1)}
	><ArrowRight size={16} aria-hidden="true" />
	</button>
	<div class="ml-auto flex items-center">
		<DownloadTasks />
	</div>
</header>

