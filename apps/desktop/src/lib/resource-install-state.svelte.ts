import { runtimeErrorCode, runtimeErrorMessage } from './runtime-errors.ts';
import { localeState } from './locale.svelte.ts';
import { createContext } from 'svelte';
import { startResourceInstall, listResourceOperations, cancelResourceOperation, RuntimeRequestError, type ResourceInstallOperation } from './runtime';

/** Window lifetime; browsing/navigation never owns the accepted background transfer. */
export class ResourceInstallState {
	operation = $state<ResourceInstallOperation | null>(null);
	starting = $state(false);
	cancelling = $state(false);
	syncing = $state(false);
	uncertain = $state(true);
	private errorCode = $state<string | null>(null);
	errorDetail = $state<string | null>(null);
	private errorKey = $state<'resourceUnknown' | 'resourceReadFailed' | 'resourceRejected' | 'resourceSubmitUnknown' | 'resourceCancelUnknown' | null>(null);
	get error() {
		if (!this.errorKey) return null;
		const fallback = localeState.messages.mods.discovery[this.errorKey];
		return this.errorKey.includes('Unknown') || this.errorKey === 'resourceUnknown' ? fallback : runtimeErrorMessage(this.errorCode, localeState.current, fallback);
	}
	bytesPerSecond = $state<number | null>(null);
	intent = $state<{ name: string; id: number } | null>(null);
	private sample: { id: number; bytes: number; at: number } | null = null;
	private timer: ReturnType<typeof setTimeout> | null = null;
	private disposed = false;
	private controllers = new Set<AbortController>();
	get active() { return this.operation?.state === 'running'; }
	get noun() { return this.operation?.kind === 'shader' ? localeState.messages.mods.discovery.resourceShader : localeState.messages.mods.discovery.resourceMod; }
	get label() {
		const copy = localeState.messages.mods;
		if (this.starting) return copy.discovery.submitting;
		if (this.cancelling || this.operation?.cancelRequested) return copy.resourceStatus(this.noun, 'cancelling');
		if (this.operation?.state === 'completed') return copy.resourceStatus(this.noun, 'completed');
		if (this.operation?.state === 'cancelled') return copy.resourceStatus(this.noun, 'cancelled');
		if (this.operation?.state === 'failed') return copy.resourceStatus(this.noun, 'failed');
		if (this.operation?.progress.stage === 'publishing') return copy.resourceStatus(this.noun, 'publishing');
		if (this.operation?.progress.stage === 'downloading') return copy.resourceStatus(this.noun, 'downloading');
		return this.operation ? copy.resourceStatus(this.noun, 'preparing') : copy.discovery.resourceTask;
	}
	canSubmit(id: number) { return !this.starting && !this.syncing && !this.active && (!this.uncertain || this.intent?.id === id); }
	private async request<T>(action: (signal: AbortSignal) => Promise<T>) {
		const controller = new AbortController(); this.controllers.add(controller);
		try { return await action(AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)])); }
		finally { this.controllers.delete(controller); }
	}
	private apply(value: ResourceInstallOperation | null) {
		if (value && this.operation && (value.id < this.operation.id || (value.id === this.operation.id && this.operation.state !== 'running' && value.state === 'running'))) return;
		if (value?.state === 'running' && value.progress.stage === 'downloading') {
			const at = performance.now(), bytes = value.progress.receivedBytes;
			if (this.sample?.id === value.id && at > this.sample.at && bytes >= this.sample.bytes) this.bytesPerSecond = (bytes - this.sample.bytes) * 1000 / (at - this.sample.at);
			this.sample = { id: value.id, at, bytes };
		} else { this.sample = null; this.bytesPerSecond = null; }
		this.operation = value;
	}
	private schedule() {
		if (this.timer) clearTimeout(this.timer);
		this.timer = null;
		if (!this.disposed && this.active) this.timer = setTimeout(() => { this.timer = null; void this.sync(); }, 1000);
	}
	async sync() {
		if (this.disposed || this.syncing || this.starting) return;
		this.syncing = true;
		try {
			const values = await this.request(listResourceOperations);
			if (this.disposed) return;
			const current = this.intent ? values.find((value) => value.id === this.intent!.id) : values.find((value) => value.state === 'running') ?? values.at(-1);
			if (this.intent && !current && this.uncertain) { this.errorKey = 'resourceUnknown'; return; }
			this.apply(current ?? null); this.uncertain = false; this.intent = null; this.errorKey = null; this.errorDetail = null; this.errorCode = null;
		} catch (error) { if (!this.disposed) { this.uncertain = true; this.errorDetail = error instanceof Error ? error.message : String(error); this.errorCode = runtimeErrorCode(error); this.errorKey = 'resourceReadFailed'; } }
		finally { this.syncing = false; this.schedule(); }
	}
	async start(name: string, id: number): Promise<boolean> {
		if (!this.canSubmit(id) || this.disposed) return false;
		this.starting = true; this.errorKey = null; this.errorDetail = null; this.errorCode = null; this.intent = { name, id };
		try {
			const value = await this.request((signal) => startResourceInstall(name, id, signal));
			if (this.disposed) return false;
			this.apply(value); this.uncertain = false; this.intent = null; return true;
		} catch (error) { if (!this.disposed) { const rejected = error instanceof RuntimeRequestError && [400, 404, 409, 503].includes(error.status); this.uncertain = !rejected; if (rejected) this.intent = null; this.errorDetail = error instanceof Error ? error.message : String(error); this.errorCode = runtimeErrorCode(error); this.errorKey = rejected ? 'resourceRejected' : 'resourceSubmitUnknown'; } return false; }
		finally { this.starting = false; if (this.uncertain && !this.disposed) void this.sync(); else this.schedule(); }
	}
	async cancel() {
		if (!this.operation || !this.active || this.cancelling || this.operation.cancelRequested) return;
		const id = this.operation.id; this.cancelling = true;
		try { const value = await this.request((signal) => cancelResourceOperation(id, signal)); if (!this.disposed && this.operation?.id === id) { this.apply(value); this.uncertain = false; this.errorKey = null; this.errorDetail = null; this.errorCode = null; } }
		catch (error) { if (!this.disposed) { this.errorDetail = error instanceof Error ? error.message : String(error); this.errorCode = runtimeErrorCode(error); this.errorKey = 'resourceCancelUnknown'; } }
		finally { this.cancelling = false; this.schedule(); }
	}
	dispose() { this.disposed = true; if (this.timer) clearTimeout(this.timer); for (const controller of this.controllers) controller.abort(); this.controllers.clear(); }
}
export const [getResourceInstallState, setResourceInstallState] = createContext<ResourceInstallState>();
