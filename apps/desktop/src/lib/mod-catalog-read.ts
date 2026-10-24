import type { CatalogOptions, CatalogStatus, CurseForgePage, ModProjectSummary } from './runtime';

/** Publish an expired snapshot immediately, then join its single background refresh. */
export async function readModCatalog<T extends CatalogStatus>(
	read: (options: CatalogOptions) => Promise<T>, publish: (value: T) => void,
	signal: AbortSignal, force = false,
) {
	let options: CatalogOptions = { refresh: force };
	for (let attempt = 0; attempt < 2; attempt++) {
		signal.throwIfAborted();
		const value = await read(options);
		signal.throwIfAborted();
		publish(value);
		if (!value.stale || value.refreshError || force) return;
		options = { refresh: true, after: value.revision };
	}
}

export interface ModProjectFeedSnapshot {
	items: ModProjectSummary[];
	nextIndex: number | null;
	busy: boolean;
	error: string | null;
	warning: string | null;
	loaded: boolean;
}
type ProjectReader = (index: number, signal: AbortSignal, options: CatalogOptions) => Promise<CurseForgePage<ModProjectSummary>>;

/** One query's bounded, cursor-based list. Only user scrolling requests subsequent pages. */
export class ModProjectFeed {
	private pages = new Map<number, CurseForgePage<ModProjectSummary>>();
	private controller: AbortController | null = null;
	private read: ProjectReader | null = null;
	private start = 0;
	private failedIndex: number | null = null;
	private snapshot: ModProjectFeedSnapshot = { items: [], nextIndex: null, busy: false, error: null, warning: null, loaded: false };
	private publish: (value: ModProjectFeedSnapshot) => void;
	constructor(publish: (value: ModProjectFeedSnapshot) => void) { this.publish = publish; }
	private emit(change: Partial<ModProjectFeedSnapshot>) {
		this.snapshot = { ...this.snapshot, ...change };
		this.publish(this.snapshot);
	}
	cancel() { this.controller?.abort(); this.controller = null; }
	reset(read: ProjectReader | null, start = 0) {
		this.cancel(); this.read = read; this.start = start; this.failedIndex = null; this.pages.clear();
		this.emit({ items: [], nextIndex: null, busy: false, error: null, warning: null, loaded: false });
		if (read) void this.load(start);
	}
	refresh() { if (!this.snapshot.busy) return this.load(this.start, true); }
	more() {
		if (!this.snapshot.busy && !this.snapshot.error && !this.snapshot.warning && this.snapshot.nextIndex !== null) return this.load(this.snapshot.nextIndex);
	}
	retry() { if (!this.snapshot.busy) return this.load(this.failedIndex ?? this.start, true); }
	private async load(index: number, force = false) {
		if (!this.read) return;
		const read = this.read;
		const controller = new AbortController();
		this.controller = controller;
		const signal = AbortSignal.any([controller.signal, AbortSignal.timeout(55_000)]);
		this.failedIndex = index;
		this.emit({ busy: true, error: null, warning: null });
		try {
			await readModCatalog(options => read(index, signal, options), value => {
				if (force && value.refreshError && this.snapshot.loaded) {
					this.emit({ warning: value.refreshError });
					return;
				}
				if (force && index === this.start) this.pages.clear();
				// A refreshed cursor must not retain pages fetched from a superseded snapshot.
				const previous = this.pages.get(index);
				if (previous && previous.nextIndex !== value.nextIndex) {
					for (const key of this.pages.keys()) if (key > index) this.pages.delete(key);
				}
				this.pages.set(index, value);
				const items = new Map<string, ModProjectSummary>();
				for (const [, page] of [...this.pages].sort(([a], [b]) => a - b)) {
					for (const item of page.items) items.set(String(item.id), item);
				}
				const last = [...this.pages].sort(([a], [b]) => b - a)[0]!;
				const next = last[1].nextIndex;
				this.emit({ items: [...items.values()], nextIndex: next !== null && next > last[0] && next <= 9980 ? next : null, warning: value.refreshError, loaded: true });
			}, signal, force);
			this.failedIndex = null;
		} catch (error) {
			if (!controller.signal.aborted) this.emit({ error: error instanceof Error ? error.message : '目录读取失败，请重试。' });
		} finally {
			if (this.controller === controller) { this.controller = null; this.emit({ busy: false }); }
		}
	}
}
