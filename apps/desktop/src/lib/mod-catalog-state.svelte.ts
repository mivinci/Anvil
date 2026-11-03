import { runtimeErrorCode, runtimeErrorMessage } from './runtime-errors.ts';
import { localeState } from './locale.svelte.ts';
import type { CatalogOptions, CatalogStatus } from './runtime';
import { readModCatalog } from './mod-catalog-read';

export class ModCatalogState<T extends CatalogStatus> {
	value = $state<T | null>(null);
	busy = $state(false);
	errorCode = $state<string | null>(null);
	errorDetail = $state<string | null>(null);
	get error() { return this.errorDetail === null ? null : runtimeErrorMessage(this.errorCode, localeState.current, localeState.messages.mods.discovery.catalogFailed); }
	private controller: AbortController | null = null;
	cancel() { this.controller?.abort(); this.controller = null; this.busy = false; }
	clear() { this.cancel(); this.value = null; this.errorDetail = null; this.errorCode = null; }
	async load(read: (signal: AbortSignal, options: CatalogOptions) => Promise<T>, force = false) {
		this.cancel();
		const controller = new AbortController();
		this.controller = controller;
		const signal = AbortSignal.any([controller.signal, AbortSignal.timeout(55_000)]);
		this.busy = true;
		this.errorDetail = null; this.errorCode = null;
		try {
			await readModCatalog(options => read(signal, options), value => { this.value = value; }, signal, force);
		} catch (error) {
			if (!controller.signal.aborted) { this.errorDetail = error instanceof Error ? error.message : String(error); this.errorCode = runtimeErrorCode(error); }
		} finally {
			if (this.controller === controller) { this.controller = null; this.busy = false; }
		}
	}
}
