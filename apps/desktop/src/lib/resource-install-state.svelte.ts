import { createContext } from 'svelte';
import { startResourceInstall, listResourceOperations, cancelResourceOperation, RuntimeRequestError, type ResourceInstallOperation } from './runtime';

/** Window lifetime; browsing/navigation never owns the accepted background transfer. */
export class ResourceInstallState {
	operation = $state<ResourceInstallOperation | null>(null);
	starting = $state(false);
	cancelling = $state(false);
	syncing = $state(false);
	uncertain = $state(true);
	error = $state<string | null>(null);
	bytesPerSecond = $state<number | null>(null);
	intent = $state<{ name: string; id: number } | null>(null);
	private sample: { id: number; bytes: number; at: number } | null = null;
	private timer: ReturnType<typeof setTimeout> | null = null;
	private disposed = false;
	private controllers = new Set<AbortController>();
	get active() { return this.operation?.state === 'running'; }
	get label() {
		if (this.starting) return '正在提交模组安装…';
		if (this.cancelling || this.operation?.cancelRequested) return '正在取消模组安装…';
		if (this.operation?.state === 'completed') return '模组安装完成';
		if (this.operation?.state === 'cancelled') return '模组安装已取消';
		if (this.operation?.state === 'failed') return '模组安装失败';
		if (this.operation?.progress.stage === 'publishing') return '正在应用模组…';
		if (this.operation?.progress.stage === 'downloading') return '正在下载并校验模组…';
		return this.operation ? '正在准备模组文件…' : '模组下载任务';
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
			if (this.intent && !current && this.uncertain) { this.error = '尚未确认提交结果，可回到原计划重试提交；同一计划不会重复安装。'; return; }
			this.apply(current ?? null); this.uncertain = false; this.intent = null; this.error = null;
		} catch (error) { if (!this.disposed) { this.uncertain = true; this.error = error instanceof Error ? error.message : '无法读取模组任务。'; } }
		finally { this.syncing = false; this.schedule(); }
	}
	async start(name: string, id: number): Promise<boolean> {
		if (!this.canSubmit(id) || this.disposed) return false;
		this.starting = true; this.error = null; this.intent = { name, id };
		try {
			const value = await this.request((signal) => startResourceInstall(name, id, signal));
			if (this.disposed) return false;
			this.apply(value); this.uncertain = false; this.intent = null; return true;
		} catch (error) { if (!this.disposed) { const rejected = error instanceof RuntimeRequestError && [400, 404, 409, 503].includes(error.status); this.uncertain = !rejected; if (rejected) this.intent = null; this.error = `${rejected ? '' : '提交结果未确认：'}${error instanceof Error ? error.message : '请刷新任务。'}`; } return false; }
		finally { this.starting = false; if (this.uncertain && !this.disposed) void this.sync(); else this.schedule(); }
	}
	async cancel() {
		if (!this.operation || !this.active || this.cancelling || this.operation.cancelRequested) return;
		const id = this.operation.id; this.cancelling = true;
		try { const value = await this.request((signal) => cancelResourceOperation(id, signal)); if (!this.disposed && this.operation?.id === id) { this.apply(value); this.uncertain = false; this.error = null; } }
		catch (error) { if (!this.disposed) this.error = error instanceof Error ? error.message : '取消结果未确认，请刷新任务。'; }
		finally { this.cancelling = false; this.schedule(); }
	}
	dispose() { this.disposed = true; if (this.timer) clearTimeout(this.timer); for (const controller of this.controllers) controller.abort(); this.controllers.clear(); }
}
export const [getResourceInstallState, setResourceInstallState] = createContext<ResourceInstallState>();
