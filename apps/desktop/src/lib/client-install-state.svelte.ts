import { createContext } from 'svelte';
import {
	listClientInstallOperations, startClientInstall, getClientInstallOperation, cancelClientInstall,
	RuntimeRequestError, pauseClientInstall, resumeClientInstall, type ClientInstallOperation, type ClientLoaderSpec,
} from './runtime';

export const installStages = {
	java: '正在准备 Java',
	resolving: '正在读取 Minecraft 版本信息', planning: '正在准备 Minecraft 安装计划', downloading: '正在下载 Minecraft',
	verifying: '正在校验 Minecraft 文件', publishing: '正在保存 Minecraft 实例', completed: 'Minecraft 安装完成',
};
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
	error = $state<string | null>(null);
	errorDetail = $state<string | null>(null);
	intent = $state<{ name: string; versionId: string } | null>(null);
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
	get canPause() { return this.operation?.state === 'running' && this.operation.progress.stage === 'downloading' && !this.operation.cancelRequested && !this.changingPause && !this.cancelling && !this.syncing && !this.uncertain; }
	get canResume() { return this.operation?.state === 'paused' && !this.operation.cancelRequested && !this.changingPause && !this.cancelling && !this.syncing && !this.uncertain; }
	get canStart() { return !this.starting && !this.syncing && !this.cancelling && !this.active && !this.changingPause && !this.retrying && !this.uncertain; }
	get canRetry() { return this.canStart && (this.operation?.state === 'failed' || this.operation?.state === 'cancelled'); }
	get canCancel() { return this.active && !this.changingPause && !this.operation?.cancelRequested && !this.cancelling && this.operation?.progress.stage !== 'publishing'; }
	get label() {
		if (this.retrying) return '正在确认重试条件…';
		if (this.starting) return '正在创建安装任务…';
		if (this.syncing) return '正在读取安装任务…';
		if (this.changingPause) return this.operation?.state === 'paused' ? '正在请求继续…' : '正在请求暂停…';
		if (this.cancelling || this.operation?.cancelRequested) return '正在取消并清理…';
		if (this.operation?.state === 'pausing') return '正在暂停传输…';
		if (this.operation?.state === 'paused') return 'Minecraft 下载已暂停';
		if (this.operation?.state === 'resuming') return '正在继续下载 Minecraft…';
		if (this.operation?.state === 'completed') return '安装完成';
		if (this.operation?.state === 'failed') return '安装失败';
		if (this.operation?.state === 'cancelled') return '安装已取消';
		if (this.operation?.progress.stage === 'java') {
			const progress = this.operation.progress, java = `Java${progress.javaMajor ? ` ${progress.javaMajor}` : ''}`;
			const steps = { resolving: '查找', downloading: '下载', extracting: '解压', validating: '校验', publishing: '安装', completed: '准备完成' };
			return progress.javaStage === 'completed' ? `${java} 已就绪` : `正在${progress.javaStage ? steps[progress.javaStage] : '准备'} ${java}…`;
		}
		return this.operation ? installStages[this.operation.progress.stage] : '暂无安装任务';
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
				this.error = '暂时无法读取进度，安装结果尚未确认，请重新查询。';
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
			if (!this.disposed) { this.uncertain = true; this.error = '无法确认安装任务状态，请重新查询后再安装。'; this.errorDetail = message(error); }
		} finally { this.syncing = false; this.schedule(); }
	}
	async start(name: string, versionId: string, loader: ClientLoaderSpec | null = null) {
		if (this.disposed || !this.canStart) return;
		this.starting = true;
		this.stopPolling();
		this.clearError();
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
				this.error = '无法创建安装任务，请检查实例名称和版本。'; this.errorDetail = message(error);
			} else {
				try {
					const operations = await this.request((signal) => listClientInstallOperations({ signal }));
					if (this.disposed) return;
					const recovered = operations.find((item) => ['running', 'pausing', 'paused', 'resuming'].includes(item.state)) ?? operations.findLast((item) => item.id > previousId && item.name === name && item.versionId === versionId && JSON.stringify(item.loader ?? null) === JSON.stringify(loader));
					if (recovered) { this.apply(recovered); this.uncertain = false; }
					else { this.uncertain = true; this.error = '提交结果尚未确认，请重新查询，不要重复安装。'; this.errorDetail = message(error); }
				} catch (recoveryError) {
					if (!this.disposed) { this.uncertain = true; this.error = '提交结果尚未确认，请重新查询，不要重复安装。'; this.errorDetail = message(recoveryError); }
				}
			}
		} finally { this.starting = false; this.schedule(); }
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
			await this.start(previous.name, previous.versionId, previous.loader ?? null);
		} finally { this.retrying = false; }
	}

	async cancel() {
		if (!this.operation || !this.canCancel || this.disposed || this.syncing) return;
		this.cancelling = true;
		this.stopPolling();
		this.clearError();
		try {
			const operation = await this.request((signal) => cancelClientInstall(this.operation!.id, { signal }));
			if (!this.disposed) { this.apply(operation); this.uncertain = false; }
		} catch (error) {
			if (!this.disposed) { this.uncertain = true; this.error = '取消结果尚未确认，请重新查询。'; this.errorDetail = message(error); }
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
				this.error = '任务状态可能已变化，请重新查询后再操作。';
				this.errorDetail = message(error);
			}
		} finally { this.changingPause = false; this.schedule(); }
	}

	dispose() {
		this.disposed = true;
		this.stopPolling();
		for (const controller of this.controllers) controller.abort();
		this.controllers.clear();
		// Page reads stop here. The Runtime owns installation and cancels on app exit.
	}
}
export const [getClientInstallState, setClientInstallState] = createContext<ClientInstallState>();
