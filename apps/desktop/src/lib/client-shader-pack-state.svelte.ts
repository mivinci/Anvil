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
	error = $state<string | null>(null);
	uncertain = $state(false);
	needsRefresh = $state(false);
	selected = $state<File | null>(null);
	removing = $state<string | null>(null);
	message = $state<string | null>(null);
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
		this.error = localPackFileError(file);
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
				this.message = activity === 'importing' ? '已添加到实例，请在游戏内选择光影。' : activity === 'removing' ? '已从此实例移除，共享原件保留。' : '受管资源文件已同步。';
				const refreshed = await refreshAfterShaderChange(change, signal => listClientShaderPacks(name, signal), AbortSignal.any([controller.signal, AbortSignal.timeout(65_000)]));
				if (!current()) return;
				if (refreshed.snapshot) { this.collection = refreshed.snapshot; this.environment = refreshed.snapshot.environment; }
				else this.error = `操作已完成，但光影环境刷新失败，请重新读取。${refreshed.error ?? ''}`;
			}
		} catch (reason) {
			if (!current()) return;
			const detail = reason instanceof Error ? reason.message : '';
			const rejected = reason instanceof RuntimeRequestError && reason.status >= 400 && reason.status < 500 && reason.status !== 408;
			if (mutation && !committed) {
				this.uncertain = !rejected;
				this.needsRefresh = !rejected || (reason instanceof RuntimeRequestError && reason.status === 409);
				this.environment = null;
				this.error = rejected ? `操作未执行。${detail}` : `操作结果未确认，请刷新清单后再操作。${detail}`;
			} else {
				this.environment = null;
				this.error = committed ? `操作已完成，但环境刷新失败，请重新读取。${detail}` : `暂时无法读取光影清单与环境。${detail}`;
			}
		} finally {
			if (this.request === controller) {
				this.request = null; this.activity = null;
				if (this.queuedRefresh) { this.queuedRefresh = false; void this.refresh(); }
			}
		}
	}
}
