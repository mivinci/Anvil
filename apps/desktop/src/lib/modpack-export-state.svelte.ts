import { requestErrorMessage } from './runtime-errors.ts';
import { localeState } from './locale.svelte.ts';
import { createContext } from 'svelte';
import {
	cancelModpackExport, createModpackExport, listModpackExports, RuntimeRequestError,
	type ModpackExportOperation, type ModpackExportOptions,
} from './runtime';

/** Window-owned export task; closing the dialog never cancels the accepted export. */
export class ModpackExportState {
	operation = $state<ModpackExportOperation | null>(null);
	starting = $state(false);
	syncing = $state(false);
	cancelling = $state(false);
	uncertain = $state(true);
	errorDetail = $state<string | null>(null);
	private errorValue = $state<string | (() => string | null) | null>(null);
	get error(): string | null { const value = this.errorValue; return typeof value === 'function' ? value() : value; }
	set error(value: string | (() => string | null) | null) {
		this.errorValue = value;
		if (value === null) this.errorDetail = null;
	}
	last = $state<{ name: string; options: ModpackExportOptions } | null>(null);
	intent = $state<{ name: string; requestId: string; options: ModpackExportOptions } | null>(null);
	private timer: ReturnType<typeof setTimeout> | null = null;
	private disposed = false;
	private controllers = new Set<AbortController>();
	get active() { return this.operation?.state === 'running'; }
	get canStart() { return !this.starting && !this.syncing && !this.active && !this.uncertain; }
	get label() {
		if (this.starting) return localeState.messages.instance.detail.creatingExport;
		if (this.cancelling || this.operation?.cancelRequested) return localeState.messages.instance.detail.cancellingExport;
		if (this.operation?.state === 'completed') return localeState.messages.instance.detail.exportCompleted;
		if (this.operation?.state === 'cancelled') return localeState.messages.instance.detail.exportCancelled;
		if (this.operation?.state === 'failed') return localeState.messages.instance.detail.exportFailed;
		if (this.operation?.progress.stage === 'collecting') return localeState.messages.instance.detail.collecting;
		if (this.operation?.progress.stage === 'packing') return localeState.messages.instance.detail.packing;
		if (this.operation?.progress.stage === 'verifying') return localeState.messages.instance.detail.verifyingPack;
		if (this.operation?.progress.stage === 'publishing') return localeState.messages.instance.detail.savingPack;
		return this.operation ? localeState.messages.instance.detail.preparingExport : localeState.messages.instance.detail.exportTask;
	}
	private async request<T>(action: (signal: AbortSignal) => Promise<T>) {
		const controller = new AbortController(); this.controllers.add(controller);
		try { return await action(AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)])); }
		finally { this.controllers.delete(controller); }
	}
	private apply(value: ModpackExportOperation | null) {
		if (value && this.operation && (value.id < this.operation.id || (value.id === this.operation.id && this.operation.state !== 'running' && value.state === 'running'))) return;
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
			const values = await this.request(listModpackExports);
			if (this.disposed) return;
			const intent = this.intent;
			if (intent) {
				const accepted = values.find(value => value.requestId === intent.requestId);
				if (accepted) { this.apply(accepted); this.intent = null; this.uncertain = false; this.error = null; }
				else if (this.uncertain) this.error = () => (localeState.messages.instance.detail.exportSubmitUnknown);
			} else {
				this.apply(values.find(value => value.state === 'running') ?? values.at(-1) ?? null);
				this.uncertain = false;
				this.error = null;
			}
		} catch (error) {
			this.errorDetail = error instanceof Error ? error.message : String(error);
			if (!this.disposed) {
				this.uncertain = true;
				this.error = () => requestErrorMessage(error, localeState.current, (localeState.messages.instance.detail.exportReadFailed));
			}
		} finally {
			this.syncing = false;
			this.schedule();
		}
	}
	private async submit(name: string, options: ModpackExportOptions, requestId: string): Promise<boolean> {
		if (this.disposed) return false;
		this.starting = true;
		this.error = null;
		this.intent = { name, requestId, options };
		this.last = { name, options };
		try {
			const value = await this.request((signal) => createModpackExport(name, requestId, options, signal));
			if (this.disposed) return false;
			this.apply(value); this.intent = null; this.uncertain = false;
			return true;
		} catch (error) {
			this.errorDetail = error instanceof Error ? error.message : String(error);
			if (this.disposed) return false;
			const rejected = error instanceof RuntimeRequestError && [400, 404, 409, 503].includes(error.status);
			this.uncertain = !rejected;
			if (rejected) this.intent = null;
			this.error = () => (rejected ? requestErrorMessage(error, localeState.current, localeState.messages.instance.exportSubmitFailed) : localeState.messages.instance.exportSubmitUnknown);
			return false;
		} finally {
			this.starting = false;
			this.schedule();
		}
	}
	async start(name: string, options: ModpackExportOptions): Promise<boolean> {
		if (!this.canStart) return false;
		this.operation = null;
		return this.submit(name, options, crypto.randomUUID());
	}
	async retry(): Promise<boolean> {
		if (this.disposed || this.starting || this.active) return false;
		if (this.intent) return this.submit(this.intent.name, this.intent.options, this.intent.requestId);
		if ((this.operation?.state === 'failed' || this.operation?.state === 'cancelled') && this.last) {
			return this.submit(this.last.name, this.last.options, crypto.randomUUID());
		}
		return false;
	}
	async cancel() {
		if (!this.operation || !this.active || this.cancelling || this.operation.cancelRequested) return;
		const id = this.operation.id;
		this.cancelling = true;
		try {
			const value = await this.request((signal) => cancelModpackExport(id, signal));
			if (!this.disposed && this.operation?.id === id) { this.apply(value); this.uncertain = false; this.error = null; }
		} catch (error) {
			this.errorDetail = error instanceof Error ? error.message : String(error);
			if (!this.disposed) this.error = () => (localeState.messages.instance.detail.cancelUnknown);
		} finally {
			this.cancelling = false;
			this.schedule();
		}
	}
	dispose() {
		this.disposed = true;
		if (this.timer) clearTimeout(this.timer);
		for (const controller of this.controllers) controller.abort();
		this.controllers.clear();
	}
}
export const [getModpackExportState, setModpackExportState] = createContext<ModpackExportState>();
