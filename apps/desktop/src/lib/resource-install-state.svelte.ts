import { runtimeErrorCode, runtimeErrorMessage } from './runtime-errors.ts';
import { localeState } from './locale.svelte.ts';
import { createContext } from 'svelte';
import { startResourceInstall, listResourceOperations, cancelResourceOperation, setResourceOperationPaused, RuntimeRequestError, type ResourceInstallOperation } from './runtime';

type TaskAction = 'pause' | 'resume' | 'cancel';
type TaskIssue = { key: 'resourceCancelUnknown' | 'resourceTransferUnknown'; detail: string };
const MAX_ACTIVE = 2;

function sameInstance(left: string, right: string) {
	const ascii = /^[\x00-\x7f]*$/;
	return ascii.test(left) === ascii.test(right) && left.toLowerCase() === right.toLowerCase();
}

/** Window lifetime; browsing/navigation never owns accepted background transfers. */
export class ResourceInstallState {
	operations = $state<ResourceInstallOperation[]>([]);
	starting = $state(false);
	syncing = $state(false);
	uncertain = $state(true);
	actions = $state<Partial<Record<number, TaskAction>>>({});
	taskIssues = $state<Partial<Record<number, TaskIssue>>>({});
	speeds = $state<Partial<Record<number, number>>>({});
	intent = $state<{ name: string; id: number } | null>(null);
	private errorCode = $state<string | null>(null);
	errorDetail = $state<string | null>(null);
	private errorKey = $state<'resourceUnknown' | 'resourceReadFailed' | 'resourceRejected' | 'resourceSubmitUnknown' | null>(null);
	private samples = new Map<number, { bytes: number; at: number }>();
	private timer: ReturnType<typeof setTimeout> | null = null;
	private disposed = false;
	private controllers = new Set<AbortController>();

	get activeOperations() { return this.operations.filter(value => value.state === 'running'); }
	get active() { return this.activeOperations.length > 0; }
	get operation() { return this.activeOperations[0] ?? this.operations.at(-1) ?? null; }
	get error() {
		if (!this.errorKey) return null;
		const fallback = localeState.messages.mods.discovery[this.errorKey];
		return this.errorKey === 'resourceRejected' || this.errorKey === 'resourceReadFailed'
			? runtimeErrorMessage(this.errorCode, localeState.current, fallback) : fallback;
	}
	byId(id: number | undefined) { return this.operations.find(value => value.id === id); }
	forClient(name: string) { return this.operations.findLast(value => sameInstance(value.name, name)); }
	busyForClient(name: string) {
		return this.activeOperations.some(value => sameInstance(value.name, name))
			|| !!(this.intent && sameInstance(this.intent.name, name));
	}
	get atCapacity() { return this.activeOperations.length >= MAX_ACTIVE; }
	canSubmit(id: number, name: string) {
		if (this.starting || this.syncing) return false;
		if (this.uncertain) return this.intent?.id === id;
		return !this.atCapacity && !this.busyForClient(name) && !this.byId(id);
	}
	nounFor(value: ResourceInstallOperation) {
		const copy = localeState.messages.mods.discovery;
		return value.kind === 'shader' ? copy.resourceShader : copy.resourceMod;
	}
	get label() {
		if (this.starting) return localeState.messages.mods.discovery.submitting;
		return this.operation ? this.labelFor(this.operation) : localeState.messages.mods.discovery.resourceTask;
	}
	labelFor(value: ResourceInstallOperation) {
		const copy = localeState.messages.mods;
		const transfer = localeState.messages.clients.flow;
		const noun = this.nounFor(value);
		const action = this.actions[value.id];
		if (action === 'cancel' || value.cancelRequested) return copy.resourceStatus(noun, 'cancelling');
		if (value.state !== 'running') return copy.resourceStatus(noun, value.state);
		if (action) return action === 'pause' ? transfer.requestPause : transfer.requestResume;
		if (value.paused) return transfer.paused;
		if (value.pauseRequested) return transfer.pausingTransfer;
		const stage = value.progress.stage;
		return copy.resourceStatus(noun, stage === 'completed' ? 'preparing' : stage);
	}
	supportsPause(value: ResourceInstallOperation) { return typeof value.paused === 'boolean' && typeof value.pauseRequested === 'boolean'; }
	private canAct(value: ResourceInstallOperation) {
		return value.state === 'running' && !this.syncing && !this.actions[value.id] && !this.taskIssues[value.id] && !value.cancelRequested;
	}
	canPause(value: ResourceInstallOperation) {
		return this.canAct(value) && !this.uncertain && this.supportsPause(value) && value.progress.stage === 'downloading' && !value.paused && !value.pauseRequested;
	}
	canResume(value: ResourceInstallOperation) {
		return this.canAct(value) && !this.uncertain && this.supportsPause(value) && !!value.paused;
	}
	canCancel(value: ResourceInstallOperation) {
		return this.canAct(value) && (value.progress.stage !== 'publishing' || !!value.paused || !!value.pauseRequested);
	}
	private async request<T>(action: (signal: AbortSignal) => Promise<T>) {
		const controller = new AbortController();
		this.controllers.add(controller);
		try { return await action(AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)])); }
		finally { this.controllers.delete(controller); }
	}
	private updateSpeed(value: ResourceInstallOperation) {
		const id = value.id;
		if (value.state === 'running' && value.progress.stage === 'downloading' && !value.paused && !value.pauseRequested) {
			const at = performance.now(), bytes = value.progress.receivedBytes;
			const sample = this.samples.get(id);
			if (sample && at > sample.at && bytes >= sample.bytes) this.speeds[id] = (bytes - sample.bytes) * 1000 / (at - sample.at);
			else delete this.speeds[id];
			this.samples.set(id, { at, bytes });
		} else this.resetSpeed(id);
	}
	private resetSpeed(id: number) {
		this.samples.delete(id);
		delete this.speeds[id];
	}
	private apply(value: ResourceInstallOperation) {
		this.updateSpeed(value);
		this.operations = [...this.operations.filter(item => item.id !== value.id), value].sort((a, b) => a.id - b.id);
	}
	private schedule() {
		if (this.timer) clearTimeout(this.timer);
		this.timer = null;
		if (!this.disposed && (this.active || Object.keys(this.taskIssues).length > 0)) this.timer = setTimeout(() => { this.timer = null; void this.sync(); }, 1000);
	}
	async sync() {
		if (this.disposed || this.syncing || this.starting || Object.keys(this.actions).length > 0) return;
		this.syncing = true;
		try {
			const values = await this.request(listResourceOperations);
			if (this.disposed) return;
			const ids = new Set(values.map(value => value.id));
			for (const id of this.samples.keys()) if (!ids.has(id)) this.resetSpeed(id);
			for (const value of values) this.updateSpeed(value);
			this.operations = [...values].sort((a, b) => a.id - b.id);
			this.taskIssues = {};
			if (this.intent && !ids.has(this.intent.id)) { this.errorKey = 'resourceUnknown'; return; }
			this.uncertain = false; this.intent = null; this.errorKey = null; this.errorDetail = null; this.errorCode = null;
		} catch (error) {
			if (!this.disposed) {
				this.uncertain = true;
				this.errorDetail = error instanceof Error ? error.message : String(error);
				this.errorCode = runtimeErrorCode(error);
				this.errorKey = 'resourceReadFailed';
			}
		} finally { this.syncing = false; this.schedule(); }
	}
	async start(name: string, id: number): Promise<boolean> {
		if (!this.canSubmit(id, name) || this.disposed) return false;
		this.starting = true; this.errorKey = null; this.errorDetail = null; this.errorCode = null; this.intent = { name, id };
		try {
			const value = await this.request((signal) => startResourceInstall(name, id, signal));
			if (this.disposed) return false;
			this.apply(value); this.uncertain = false; this.intent = null; return true;
		} catch (error) {
			if (!this.disposed) {
				const rejected = error instanceof RuntimeRequestError && [400, 404, 409, 503].includes(error.status);
				this.uncertain = !rejected;
				if (rejected) this.intent = null;
				this.errorDetail = error instanceof Error ? error.message : String(error);
				this.errorCode = runtimeErrorCode(error);
				this.errorKey = rejected ? 'resourceRejected' : 'resourceSubmitUnknown';
			}
			return false;
		} finally { this.starting = false; if (this.uncertain && !this.disposed) void this.sync(); else this.schedule(); }
	}
	cancel(id: number) { return this.changeTask(id, 'cancel'); }
	setPaused(id: number, paused: boolean) { return this.changeTask(id, paused ? 'pause' : 'resume'); }
	private async changeTask(id: number, action: TaskAction) {
		const value = this.byId(id);
		if (this.disposed || !value || !(action === 'cancel' ? this.canCancel(value) : action === 'pause' ? this.canPause(value) : this.canResume(value))) return;
		this.actions[id] = action;
		this.resetSpeed(id);
		try {
			const updated = await this.request(signal => action === 'cancel'
				? cancelResourceOperation(id, signal)
				: setResourceOperationPaused(id, action === 'pause', signal));
			if (!this.disposed) this.apply(updated);
		} catch (error) {
			if (!this.disposed) this.taskIssues[id] = {
				key: action === 'cancel' ? 'resourceCancelUnknown' : 'resourceTransferUnknown',
				detail: error instanceof Error ? error.message : String(error),
			};
		} finally { delete this.actions[id]; this.schedule(); }
	}
	dispose() {
		this.disposed = true;
		if (this.timer) clearTimeout(this.timer);
		for (const controller of this.controllers) controller.abort();
		this.controllers.clear();
	}
}
export const [getResourceInstallState, setResourceInstallState] = createContext<ResourceInstallState>();
