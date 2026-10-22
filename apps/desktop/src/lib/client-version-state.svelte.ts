import { getContext, setContext } from 'svelte';
import { listClientVersionCatalog, type GameVersion } from './runtime';

const KEY = Symbol('client-versions');
const TTL = 30 * 60 * 1000;

/** Window-owned catalogue: dialogs reuse data and one request, without polling. */
export class ClientVersionState {
	versions = $state<GameVersion[]>([]);
	loaded = $state(false);
	loading = $state(false);
	error = $state<string | null>(null);
	private freshUntil = 0;
	private request: AbortController | null = null;
	private disposed = false;

	async load(force = false) {
		if (this.disposed || this.request || (!force && this.loaded && performance.now() < this.freshUntil)) return;
		const controller = new AbortController();
		this.request = controller;
		this.loading = true;
		this.error = null;
		try {
			let refresh = force;
			let after: number | undefined;
			for (let attempt = 0; attempt < 2; attempt++) {
				const result = await listClientVersionCatalog({ refresh, ...(after !== undefined ? { after } : {}), signal: AbortSignal.any([controller.signal, AbortSignal.timeout(35_000)]) });
				if (this.disposed) return;
				if (!Array.isArray(result.versions) || result.versions.some((item) => !item || typeof item.id !== 'string' || typeof item.type !== 'string' || typeof item.releaseTime !== 'string') || typeof result.stale !== 'boolean' || !Number.isFinite(result.freshForMs) || !Number.isSafeInteger(result.revision)) throw new Error('版本目录格式不正确。');
				this.versions = result.versions.filter((item) => /^[a-zA-Z0-9][a-zA-Z0-9._+ -]{0,63}$/.test(item.id) && !/[ .]$/.test(item.id)).sort((a, b) => b.releaseTime.localeCompare(a.releaseTime));
				this.loaded = true;
				this.error = result.refreshError;
				this.freshUntil = performance.now() + Math.max(0, Math.min(TTL, result.freshForMs));
				if (refresh || !result.stale || result.refreshError) break;
				// Stale data is already visible; join the Runtime's background refresh.
				refresh = true;
				after = result.revision;
			}
		} catch (error) {
			if (!this.disposed) this.error = error instanceof Error ? error.message : String(error);
		} finally {
			if (this.request === controller) this.request = null;
			if (!this.disposed) this.loading = false;
		}
	}

	dispose() { this.disposed = true; this.request?.abort(); }
}
export const setClientVersionState = (state: ClientVersionState) => setContext(KEY, state);
export const getClientVersionState = () => getContext<ClientVersionState>(KEY);
