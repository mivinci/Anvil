import type { CatalogOptions, CatalogStatus } from './runtime';
import { readModCatalog } from './mod-catalog-read';

export class ModCatalogState<T extends CatalogStatus> {
	value = $state<T | null>(null);
	busy = $state(false);
	error = $state<string | null>(null);
	private controller: AbortController | null = null;
	cancel() { this.controller?.abort(); this.controller = null; this.busy = false; }
	clear() { this.cancel(); this.value = null; this.error = null; }
	async load(read: (signal: AbortSignal, options: CatalogOptions) => Promise<T>, force = false) {
		this.cancel();
		const controller = new AbortController();
		this.controller = controller;
		const signal = AbortSignal.any([controller.signal, AbortSignal.timeout(55_000)]);
		this.busy = true;
		this.error = null;
		try {
			await readModCatalog(options => read(signal, options), value => { this.value = value; }, signal, force);
		} catch (error) {
			if (!controller.signal.aborted) this.error = error instanceof Error ? error.message : '目录读取失败，请重试。';
		} finally {
			if (this.controller === controller) { this.controller = null; this.busy = false; }
		}
	}
}
