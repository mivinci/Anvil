import { requestErrorMessage } from './runtime-errors.ts';
import { localeState } from './locale.svelte.ts';
import {
	listClientShaderPacks, importClientShaderPack, removeClientShaderPack, syncClientResources, RuntimeRequestError,
	type ClientShaderPackChange, type ClientShaderPack, type ShaderEnvironment,
} from './runtime';
import { localPackFileError } from './local-pack';
import { validateShaderPackChange, validateShaderPacks, refreshAfterShaderChange } from './shader-packs';

/** Page-local state. Explicit writes, no polling or automatic mutation retries. */
export class ClientShaderPackState {
	collection = $state<ClientShaderPackChange | null>(null);
	environment = $state<ShaderEnvironment | null>(null);
	activity = $state<'loading' | 'importing' | 'removing' | 'syncing' | null>(null);
	errorDetail = $state<string | null>(null);
	private errorValue = $state<string | (() => string | null) | null>(null);
	get error(): string | null { const value = this.errorValue; return typeof value === 'function' ? value() : value; }
	set error(value: string | (() => string | null) | null) {
		this.errorValue = value;
		if (value === null) this.errorDetail = null;
	}
	uncertain = $state(false);
	needsRefresh = $state(false);
	selected = $state<File | null>(null);
	removing = $state<string | null>(null);
	private messageValue = $state<string | (() => string | null) | null>(null);
	get message(): string | null { const value = this.messageValue; return typeof value === 'function' ? value() : value; }
	set message(value: string | (() => string | null) | null) { this.messageValue = value; }
	readonly busy = $derived(this.activity !== null);
	readonly blocked = $derived(this.uncertain || this.needsRefresh);
	private name = '';
	private request: AbortController | null = null;
	private queuedRefresh = false;

	open(name: string) {
		this.dispose(); this.name = name;
		this.collection = null; this.environment = null; this.selected = null; this.removing = null;
		this.error = null; this.uncertain = false; this.needsRefresh = false; this.message = null;
		void this.refresh();
	}
	dispose() { this.request?.abort(); this.request = null; this.activity = null; this.queuedRefresh = false; }
	queueRefresh() { if (this.busy) this.queuedRefresh = true; else void this.refresh(); }
	async refresh() {
		const name = this.name;
		await this.run(signal => listClientShaderPacks(name, signal), 'loading');
	}
	pick(file: File) {
		if (this.busy || this.blocked || !this.collection) return;
		this.selected = null; this.removing = null; this.message = null;
		this.error = () => localPackFileError(file, localeState.current);
		if (!this.error) this.selected = file;
	}
	async importSelected() {
		if (!this.collection || !this.selected || this.busy || this.blocked) return;
		const name = this.name, file = this.selected, revision = this.collection.revision;
		await this.run(signal => importClientShaderPack(name, file, revision, signal), 'importing');
	}
	async remove(pack: ClientShaderPack) {
		if (!this.collection || this.busy || this.blocked || this.removing !== pack.sha256) return;
		const name = this.name, revision = this.collection.revision;
		await this.run(signal => removeClientShaderPack(name, pack.sha256, revision, signal), 'removing');
	}
	async sync() {
		if (!this.collection || this.busy || this.blocked) return;
		const name = this.name, revision = this.collection.revision;
		await this.run(async signal => {
			const result = await syncClientResources(name, revision, signal);
			return { revision: result.revision, appliedRevision: result.appliedRevision ?? null, packs: result.shaderPacks ?? [] };
		}, 'syncing');
	}
	private async run(action: (signal: AbortSignal) => Promise<ClientShaderPackChange>, activity: NonNullable<ClientShaderPackState['activity']>) {
		if (this.request) return;
		const controller = new AbortController(), name = this.name; this.request = controller;
		const current = () => this.request === controller && !controller.signal.aborted;
		const mutation = activity !== 'loading';
		let committed = false;
		this.activity = activity; this.error = null; this.message = null;
		try {
			const signal = AbortSignal.any([controller.signal, AbortSignal.timeout(65_000)]);
			const value = await action(signal);
			if (!current()) return;
			if (!mutation) {
				const snapshot = validateShaderPacks(value as import('./runtime').ClientShaderPacks);
				this.collection = snapshot; this.environment = snapshot.environment;
				if (this.blocked) { this.selected = null; this.removing = null; }
				this.uncertain = false; this.needsRefresh = false;
			} else {
				const change = validateShaderPackChange(value);
				committed = true;
				this.collection = change; this.environment = null;
				this.selected = null; this.removing = null; this.uncertain = false; this.needsRefresh = false;
				this.message = () => (activity === 'importing' ? localeState.messages.instance.detail.shaderAdded : activity === 'removing' ? localeState.messages.instance.detail.removed : localeState.messages.instance.detail.resourcesSynced);
				const refreshed = await refreshAfterShaderChange(change, signal => listClientShaderPacks(name, signal), AbortSignal.any([controller.signal, AbortSignal.timeout(65_000)]));
				if (!current()) return;
				if (refreshed.snapshot) { this.collection = refreshed.snapshot; this.environment = refreshed.snapshot.environment; }
				else { this.errorDetail = refreshed.error; this.error = () => (localeState.messages.instance.shaderRefreshFailed); }
			}
		} catch (reason) {
			if (!current()) return;
			const detail = reason instanceof Error ? reason.message : ''; this.errorDetail = detail;
			const rejected = reason instanceof RuntimeRequestError && reason.status >= 400 && reason.status < 500 && reason.status !== 408;
			if (mutation && !committed) {
				this.uncertain = !rejected;
				this.needsRefresh = !rejected || (reason instanceof RuntimeRequestError && reason.status === 409);
				this.environment = null;
				this.error = () => (rejected ? requestErrorMessage(reason, localeState.current, localeState.messages.instance.mutationRejected) : localeState.messages.instance.mutationUnknown);
			} else {
				this.environment = null;
				this.error = () => requestErrorMessage(reason, localeState.current, (committed ? localeState.messages.instance.shaderRefreshFailed : localeState.messages.instance.shadersReadFailed));
			}
		} finally {
			if (this.request === controller) {
				this.request = null; this.activity = null;
				if (this.queuedRefresh) { this.queuedRefresh = false; void this.refresh(); }
			}
		}
	}
}
