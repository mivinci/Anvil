import { requestErrorMessage } from './runtime-errors.ts';
import { localeState } from './locale.svelte.ts';
import { createContext } from 'svelte';
import { deleteClient, getClientDetails, RuntimeRequestError } from './runtime';

/** Window ownership preserves an unresolved destructive request across navigation. */
export class ClientRemovalState {
	name = $state<string | null>(null);
	busy = $state(false);
	uncertain = $state(false);
	errorDetail = $state<string | null>(null);
	private errorValue = $state<string | (() => string) | null>(null);
	get error(): string | null { const value = this.errorValue; return typeof value === 'function' ? value() : value; }
	set error(value: string | (() => string) | null) { this.errorValue = value; }
	private controller: AbortController | null = null;
	private disposed = false;
	constructor(private removed: (name: string, cleanupPending: boolean) => void) {}
	private complete(cleanupPending: boolean) {
		if (this.name && !this.disposed) this.removed(this.name, cleanupPending);
		this.name = null; this.error = null; this.errorDetail = null; this.uncertain = false;
		return true;
	}
	async remove(name: string) {
		if (this.busy || this.uncertain || this.disposed) return false;
		this.name = name; this.busy = true; this.error = null; this.errorDetail = null;
		const controller = new AbortController(); this.controller = controller;
		try {
			const result = await deleteClient(name, AbortSignal.any([controller.signal, AbortSignal.timeout(30_000)]));
			if (this.disposed) return false;
			if (result.removed !== true || typeof result.cleanupPending !== 'boolean') throw new Error('invalid removal receipt');
			return this.complete(result.cleanupPending);
		} catch (error) {
			this.errorDetail = error instanceof Error ? error.message : String(error);
			if (this.disposed) return false;
			if (error instanceof RuntimeRequestError && error.status === 404) return this.complete(true);
			this.uncertain = !(error instanceof RuntimeRequestError && error.status >= 400 && error.status < 500 && error.status !== 408);
			this.error = () => (this.uncertain ? localeState.messages.clients.flow.removeUnknown : localeState.messages.clients.flow.removeFailed);
			return false;
		} finally { this.busy = false; this.controller = null; }
	}
	async reconcile() {
		if (!this.name || this.busy || this.disposed) return false;
		this.busy = true;
		const controller = new AbortController(); this.controller = controller;
		try {
			await getClientDetails(this.name, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (!this.disposed) this.error = () => (localeState.messages.clients.flow.removePending);
			return false;
		} catch (error) {
			this.errorDetail = error instanceof Error ? error.message : String(error);
			if (!this.disposed && error instanceof RuntimeRequestError && error.status === 404) return this.complete(true);
			if (!this.disposed) this.error = () => requestErrorMessage(error, localeState.current, (localeState.messages.clients.flow.removeCheckFailed));
			return false;
		} finally { this.busy = false; this.controller = null; }
	}
	dispose() { this.disposed = true; this.controller?.abort(); }
}
export const [getClientRemovalState, setClientRemovalState] = createContext<ClientRemovalState>();
