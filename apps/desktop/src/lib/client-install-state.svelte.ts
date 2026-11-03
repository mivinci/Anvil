import { requestErrorMessage } from './runtime-errors.ts';
import { localeState } from './locale.svelte.ts';
import { createContext } from 'svelte';
import {
	listClientInstallOperations, startClientInstall, importClientModpack, getClientInstallOperation, cancelClientInstall,
	RuntimeRequestError, pauseClientInstall, resumeClientInstall, type ClientInstallOperation, type ClientLoaderSpec,
} from './runtime';

export const installStages = () => ({
	uploading: localeState.messages.clients.flow.readingPack,
	parsing: localeState.messages.clients.flow.parsingPack, applying: localeState.messages.clients.flow.applyingPack,
	java: localeState.messages.clients.flow.preparingJava,
	resolving: localeState.messages.clients.flow.resolvingMinecraft, planning: localeState.messages.clients.flow.planningMinecraft, downloading: localeState.messages.clients.flow.downloadingMinecraft,
	verifying: localeState.messages.clients.flow.verifyingMinecraft, publishing: localeState.messages.clients.flow.publishingMinecraft, completed: localeState.messages.clients.flow.installed,
});
const message = (error: unknown) => error instanceof Error ? error.message : String(error);
export class ClientInstallState {
	operation = $state<ClientInstallOperation | null>(null);
	starting = $state(false);
	syncing = $state(false);
	cancelling = $state(false);
	changingPause = $state(false);
	retrying = $state(false);
	uncertain = $state(true);
	detailsOpen = $state(false);
	bytesPerSecond = $state<number | null>(null);
	private speedSample: { id: number; bytes: number; time: number } | null = null;
	private errorValue = $state<string | (() => string) | null>(null);
	get error(): string | null { const value = this.errorValue; return typeof value === 'function' ? value() : value; }
	set error(value: string | (() => string) | null) { this.errorValue = value; }
	errorDetail = $state<string | null>(null);
	intent = $state<{ name: string; versionId: string } | null>(null);
	importFile = $state<File | null>(null);
	private importRequestId: string | null = null;
	private uploadController: AbortController | null = null;
	get importing() { return !!this.importFile || !!this.operation?.importRequestId; }
	private disposed = false;
	private timer: ReturnType<typeof setTimeout> | null = null;
	private pollController: AbortController | null = null;
	private controllers = new Set<AbortController>();
	private generation = 0;
	private latestId = 0;
	private completedId = 0;
	constructor(private readonly onCompleted: (receipt: NonNullable<ClientInstallOperation['result']>) => void = () => {}) {}
	get active() { return !!this.operation && ['running', 'pausing', 'paused', 'resuming'].includes(this.operation.state); }
	get running() { return !!this.operation && (['running', 'pausing', 'resuming'].includes(this.operation.state) || (this.operation.state === 'paused' && this.operation.cancelRequested)); }
	get canPause() { return !this.operation?.importRequestId && this.operation?.state === 'running' && this.operation.progress.stage === 'downloading' && !this.operation.cancelRequested && !this.changingPause && !this.cancelling && !this.syncing && !this.uncertain; }
	get canResume() { return this.operation?.state === 'paused' && !this.operation.cancelRequested && !this.changingPause && !this.cancelling && !this.syncing && !this.uncertain; }
	get canStart() { return !this.starting && !this.syncing && !this.cancelling && !this.active && !this.changingPause && !this.retrying && !this.uncertain; }
	get canRetry() { return (!this.operation?.importRequestId || !!this.importFile) && this.canStart && (this.operation?.state === 'failed' || this.operation?.state === 'cancelled'); }
	get canCancel() { if (this.starting && this.uploadController) return true; return this.active && !this.changingPause && !this.operation?.cancelRequested && !this.cancelling && this.operation?.progress.stage !== 'publishing'; }
	get label() {
		if (this.retrying) return localeState.messages.clients.flow.checkingRetry;
		if (this.starting) return this.importing ? localeState.messages.clients.flow.readingPack : localeState.messages.clients.flow.creatingTask;
		if (this.syncing) return localeState.messages.clients.flow.loadingTask;
		if (this.changingPause) return this.operation?.state === 'paused' ? localeState.messages.clients.flow.requestResume : localeState.messages.clients.flow.requestPause;
		if (this.cancelling || this.operation?.cancelRequested) return localeState.messages.clients.flow.cancelling;
		if (this.operation?.state === 'pausing') return localeState.messages.clients.flow.pausingTransfer;
		if (this.operation?.state === 'paused') return localeState.messages.clients.flow.minecraftPaused;
		if (this.operation?.state === 'resuming') return localeState.messages.clients.flow.resumingMinecraft;
		if (this.operation?.state === 'completed') return localeState.messages.clients.flow.installComplete;
		if (this.operation?.state === 'failed') return localeState.messages.clients.flow.installFailed;
		if (this.operation?.state === 'cancelled') return localeState.messages.clients.flow.installCancelled;
		if (this.operation?.progress.stage === 'java') {
			const progress = this.operation.progress, java = `Java${progress.javaMajor ? ` ${progress.javaMajor}` : ''}`;
			return localeState.messages.clients.javaStage(progress.javaStage, java);
		}
		if (this.operation?.importRequestId && this.operation.progress.stage === 'downloading') return localeState.messages.clients.flow.downloadingPack;
		if (this.operation?.progress.stage === 'downloading') {
			return this.operation.progress.receivedBytes > 0 ? localeState.messages.clients.flow.downloadingMissing : localeState.messages.clients.flow.reusingMinecraft;
		}
		return this.operation ? installStages()[this.operation.progress.stage] : localeState.messages.clients.flow.noInstall;
	}
	private async request<T>(run: (signal: AbortSignal) => Promise<T>): Promise<T> {
		const controller = new AbortController();
		this.controllers.add(controller);
		try { return await run(AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)])); }
		finally { this.controllers.delete(controller); }
	}
	private clearError() { this.error = null; this.errorDetail = null; }
	private apply(operation: ClientInstallOperation | null) {
		const measuring = operation?.state === 'running' && operation.progress.stage === 'downloading' && !operation.cancelRequested;
		if (measuring && operation) {
			const now = performance.now();
			const bytes = operation.progress.receivedBytes ?? 0;
			const sample = this.speedSample;
			if (!sample || sample.id !== operation.id || bytes < sample.bytes) {
				this.bytesPerSecond = null;
				this.speedSample = { id: operation.id, bytes, time: now };
			} else if (now - sample.time >= 500) {
				this.bytesPerSecond = (bytes - sample.bytes) * 1_000 / (now - sample.time);
				this.speedSample = { id: operation.id, bytes, time: now };
			}
		} else { this.bytesPerSecond = null; this.speedSample = null; }
		this.operation = operation;
		if (operation) this.latestId = Math.max(this.latestId, operation.id);
		if (operation?.state === 'completed' && operation.result && operation.id > this.completedId) {
			this.completedId = operation.id;
			this.onCompleted(operation.result);
			this.importFile = null;
		}
		if (!this.running) this.stopPolling();
	}
	private stopPolling() {
		this.generation++;
		if (this.timer) clearTimeout(this.timer);
		this.timer = null;
		this.pollController?.abort();
		this.pollController = null;
	}
	private schedule() {
		if (this.disposed || !this.running || this.timer || this.pollController || this.error || this.syncing || this.starting || this.cancelling || this.changingPause) return;
		const generation = this.generation;
		this.timer = setTimeout(() => { this.timer = null; void this.poll(generation); }, 1_200);
	}
	private async poll(generation: number) {
		if (!this.operation || !this.running || this.disposed || generation !== this.generation) return;
		const controller = new AbortController();
		this.pollController = controller;
		let success = false;
		try {
			const operation = await getClientInstallOperation(this.operation.id, { signal: AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]) });
			if (this.disposed || generation !== this.generation) return;
			this.apply(operation);
			success = true;
		} catch (error) {
			if (!this.disposed && generation === this.generation) {
				this.bytesPerSecond = null;
				this.speedSample = null;
				this.error = () => (localeState.messages.clients.flow.progressUnknown);
				this.errorDetail = message(error);
				this.uncertain = true;
			}
		} finally {
			if (this.pollController === controller) this.pollController = null;
			if (success) this.schedule();
		}
	}
	async sync() {
		if (this.disposed || this.starting || this.syncing || this.cancelling || this.changingPause) return;
		this.syncing = true;
		this.stopPolling();
		this.clearError();
		try {
			const operations = await this.request((signal) => listClientInstallOperations({ signal }));
			if (this.disposed) return;
			const active = operations.find((operation) => ['running', 'pausing', 'paused', 'resuming'].includes(operation.state));
			const known = operations.find((operation) => operation.id === this.operation?.id);
			this.apply(active ?? known ?? operations.at(-1) ?? null);
			this.uncertain = false;
		} catch (error) {
			if (!this.disposed) { this.uncertain = true; this.error = () => (localeState.messages.clients.flow.installStateUnknown); this.errorDetail = message(error); }
		} finally { this.syncing = false; this.schedule(); }
	}
	async start(name: string, versionId: string, loader: ClientLoaderSpec | null = null) {
		if (this.disposed || !this.canStart) return;
		this.starting = true;
		this.stopPolling();
		this.clearError();
		this.importFile = null;
		this.importRequestId = null;
		this.intent = { name, versionId };
		this.operation = null;
		this.bytesPerSecond = null;
		this.speedSample = null;
		const previousId = this.latestId;
		try {
			const operation = await this.request((signal) => startClientInstall(name, versionId, { signal, loader }));
			if (!this.disposed) { this.apply(operation); this.uncertain = false; }
		} catch (error) {
			if (this.disposed) return;
			if (error instanceof RuntimeRequestError && error.status >= 400 && error.status < 500 && error.status !== 409) {
				this.error = () => requestErrorMessage(error, localeState.current, (localeState.messages.clients.flow.createFailed)); this.errorDetail = message(error);
			} else {
				try {
					const operations = await this.request((signal) => listClientInstallOperations({ signal }));
					if (this.disposed) return;
					const recovered = operations.find((item) => ['running', 'pausing', 'paused', 'resuming'].includes(item.state)) ?? operations.findLast((item) => item.id > previousId && item.name === name && item.versionId === versionId && JSON.stringify(item.loader ?? null) === JSON.stringify(loader));
					if (recovered) { this.apply(recovered); this.uncertain = false; }
					else { this.uncertain = true; this.error = () => (localeState.messages.clients.flow.submitUnknown); this.errorDetail = message(error); }
				} catch (recoveryError) {
					if (!this.disposed) { this.uncertain = true; this.error = () => (localeState.messages.clients.flow.submitUnknown); this.errorDetail = message(recoveryError); }
				}
			}
		} finally { this.starting = false; this.schedule(); }
	}
	async importPack(file: File) {
		if (this.disposed || !this.canStart) return;
		if (!file.name.toLowerCase().endsWith('.mrpack') || file.size === 0 || file.size > 512 * 1024 * 1024) {
			this.operation = null; this.importFile = null; this.intent = { name: file.name, versionId: 'unknown' };
			this.error = () => (localeState.messages.clients.flow.invalidPack); return;
		}
		this.starting = true; this.stopPolling(); this.clearError();
		this.importFile = file; this.importRequestId = crypto.randomUUID();
		this.intent = { name: file.name.replace(/\.mrpack$/i, ''), versionId: 'unknown' };
		this.operation = null;
		const requestId = this.importRequestId;
		const controller = new AbortController(); this.uploadController = controller;
		try {
			const operation = await importClientModpack(file, requestId, AbortSignal.any([controller.signal, AbortSignal.timeout(60_000)]), (operation) => { if (!this.disposed) this.apply(operation); });
			if (!this.disposed) { this.apply(operation); this.uncertain = false; }
		} catch (error) {
			if (this.disposed) return;
			try {
				const operations = await this.request((signal) => listClientInstallOperations({ signal }));
				if (this.disposed) return;
				const accepted = operations.find(op => op.importRequestId === requestId);
				if (accepted) {
					this.apply(accepted); this.uncertain = false;
					if (controller.signal.aborted || accepted.progress.stage === 'uploading') {
						const stopped = await this.request(signal => cancelClientInstall(accepted.id, { signal }));
						if (!this.disposed) this.apply(stopped);
					}
				}
				else { this.uncertain = false; this.error = () => requestErrorMessage(error, localeState.current, (controller.signal.aborted ? localeState.messages.clients.flow.uploadCancelled : localeState.messages.clients.flow.uploadFailed)); this.errorDetail = message(error); }
			} catch (recoveryError) { this.uncertain = true; this.error = () => (localeState.messages.clients.flow.importUnknown); this.errorDetail = message(recoveryError); }
		} finally { this.uploadController = null; this.starting = false; this.schedule(); }
	}
	async retry() {
		if (this.disposed || this.retrying || !this.canRetry || !this.operation) return;
		const previous = this.operation;
		this.retrying = true;
		try {
			// Re-read the authoritative task before creating another one.
			await this.sync();
			if (this.disposed) return;
			this.retrying = false;
			if (!this.canRetry || this.operation?.id !== previous.id) return;
			if (previous.importRequestId) {
				if (this.importFile) await this.importPack(this.importFile);
			} else await this.start(previous.name, previous.versionId, previous.loader ?? null);
		} finally { this.retrying = false; }
	}

	async cancel() {
		if (this.starting && this.uploadController) { this.uploadController.abort(); return; }
		if (!this.operation || !this.canCancel || this.disposed || this.syncing) return;
		this.cancelling = true;
		this.stopPolling();
		this.clearError();
		try {
			const operation = await this.request((signal) => cancelClientInstall(this.operation!.id, { signal }));
			if (!this.disposed) { this.apply(operation); this.uncertain = false; }
		} catch (error) {
			if (!this.disposed) { this.uncertain = true; this.error = () => (localeState.messages.clients.flow.cancelUnknown); this.errorDetail = message(error); }
		} finally { this.cancelling = false; this.schedule(); }
	}
	async setPaused(paused: boolean) {
		if (this.disposed || !this.operation || (paused ? !this.canPause : !this.canResume)) return;
		this.changingPause = true;
		this.stopPolling();
		this.clearError();
		const id = this.operation.id;
		try {
			const operation = await this.request((signal) => (paused ? pauseClientInstall : resumeClientInstall)(id, { signal }));
			if (!this.disposed) { this.apply(operation); this.uncertain = false; }
		} catch (error) {
			if (!this.disposed) {
				this.uncertain = true;
				this.error = () => requestErrorMessage(error, localeState.current, (localeState.messages.clients.flow.stateChanged));
				this.errorDetail = message(error);
			}
		} finally { this.changingPause = false; this.schedule(); }
	}

	dispose() {
		this.disposed = true;
		this.stopPolling();
		this.uploadController?.abort();
		for (const controller of this.controllers) controller.abort();
		this.controllers.clear();
		// Page reads stop here. The Runtime owns installation and cancels on app exit.
	}
}
export const [getClientInstallState, setClientInstallState] = createContext<ClientInstallState>();
