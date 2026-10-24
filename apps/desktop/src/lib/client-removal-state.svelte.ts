import { createContext } from 'svelte';
import { deleteClient, getClientDetails, RuntimeRequestError } from './runtime';

/** Window ownership preserves an unresolved destructive request across navigation. */
export class ClientRemovalState {
	name = $state<string | null>(null);
	busy = $state(false);
	uncertain = $state(false);
	error = $state<string | null>(null);
	private controller: AbortController | null = null;
	private disposed = false;
	constructor(private removed: (name: string, cleanupPending: boolean) => void) {}
	private complete(cleanupPending: boolean) {
		if (this.name && !this.disposed) this.removed(this.name, cleanupPending);
		this.name = null; this.error = null; this.uncertain = false;
		return true;
	}
	async remove(name: string) {
		if (this.busy || this.uncertain || this.disposed) return false;
		this.name = name; this.busy = true; this.error = null;
		const controller = new AbortController(); this.controller = controller;
		try {
			const result = await deleteClient(name, AbortSignal.any([controller.signal, AbortSignal.timeout(30_000)]));
			if (this.disposed) return false;
			if (result.removed !== true || typeof result.cleanupPending !== 'boolean') throw new Error('invalid removal receipt');
			return this.complete(result.cleanupPending);
		} catch (error) {
			if (this.disposed) return false;
			if (error instanceof RuntimeRequestError && error.status === 404) return this.complete(true);
			this.uncertain = !(error instanceof RuntimeRequestError && error.status >= 400 && error.status < 500 && error.status !== 408);
			this.error = this.uncertain ? '删除结果尚未确认，请先刷新结果，不要重复提交。' : error instanceof Error ? error.message : '删除未完成，请重试。';
			return false;
		} finally { this.busy = false; this.controller = null; }
	}
	async reconcile() {
		if (!this.name || this.busy || this.disposed) return false;
		this.busy = true;
		const controller = new AbortController(); this.controller = controller;
		try {
			await getClientDetails(this.name, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (!this.disposed) this.error = '实例仍存在或删除尚在进行，请稍后刷新结果。';
			return false;
		} catch (error) {
			if (!this.disposed && error instanceof RuntimeRequestError && error.status === 404) return this.complete(true);
			if (!this.disposed) this.error = '暂时无法确认删除结果，请稍后刷新。';
			return false;
		} finally { this.busy = false; this.controller = null; }
	}
	dispose() { this.disposed = true; this.controller?.abort(); }
}
export const [getClientRemovalState, setClientRemovalState] = createContext<ClientRemovalState>();
