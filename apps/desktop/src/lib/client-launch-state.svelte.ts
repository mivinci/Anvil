import { createContext } from 'svelte';
import { readRecentPlays, recordRecentPlay, forgetRecentPlay, clearRecentPlays } from './recent-plays';
import { listClientLaunches, startClientLaunch, stopClientLaunch, type ClientLaunch } from './runtime';
export const launchActive = (value: ClientLaunch | undefined) => !!value && !['exited', 'failed', 'cancelled'].includes(value.phase);
export const launchNeedsLogin = (value: ClientLaunch | undefined) =>
	value?.phase === 'failed' && !value.offlineTest &&
	(value.errorCode === 'account_required' || value.errorCode === 'authentication_failed');
export function launchLabel(value: ClientLaunch): string {
	if (value.cancelRequested) return '正在取消启动…';
	const java = `Java${value.javaMajor ? ` ${value.javaMajor}` : ''}`;
	switch (value.phase) {
		case 'preparing': return '正在准备启动…';
		case 'authenticating': return '正在验证账户…';
		case 'planning': return '正在读取 Minecraft 启动信息…';
		case 'java':
			switch (value.javaStage) {
				case 'resolving': return `正在准备 ${java}…`;
				case 'downloading': return `正在下载 ${java}…`;
				case 'extracting': return `正在解压 ${java}…`;
				case 'validating': return `正在检查 ${java}…`;
				case 'publishing': return `正在安装 ${java}…`;
				case 'completed': return '正在启动游戏…';
				default: return '正在检查 Minecraft 文件…';
			}
		case 'running': return value.offlineTest ? '离线测试运行中' : '运行中';
		case 'stopping': return '正在结束游戏…';
		case 'failed': return value.error ?? '启动失败';
		case 'cancelled': return '启动已取消';
		case 'exited': return '游戏已退出';
	}
}
export class ClientLaunchState {
	recentPlays = $state(readRecentPlays());
	clearRecentHistory(): boolean { this.recentPlays = []; return clearRecentPlays(); }
	private awaitingRecent = new Set<string>();
	forgetClient(name: string) {
		this.recentPlays = forgetRecentPlay(this.recentPlays, name);
		this.operations = this.operations.filter(value => value.name !== name.toLowerCase());
		this.awaitingRecent.delete(name.toLowerCase());
	}
	operations = $state<ClientLaunch[]>([]);
	pending = $state<string[]>([]);
	uncertain = $state(true);
	error = $state<string | null>(null);
	private timer: ReturnType<typeof setTimeout> | null = null;
	private disposed = false;
	private generation = 0;
	private readSequence = 0;
	private controllers = new Set<AbortController>();
	get javaTasks() {
		// Keep the last real Java installation per instance, including its terminal state.
		// Fast probes of an already managed runtime never create a download task.
		return [...new Map(this.operations.filter(value => value.javaDownloadStarted).map(value => [value.name, value])).values()];
	}
	forClient(name: string) { return this.operations.filter(v => v.name === name.toLowerCase()).at(-1); }
	private controller() { const value = new AbortController(); this.controllers.add(value); return value; }
	private recordPlay(value: ClientLaunch) {
		const name = value.name.toLowerCase();
		if (!this.awaitingRecent.has(name)) return;
		if (value.phase === 'running') {
			this.recentPlays = recordRecentPlay(this.recentPlays, value.name);
			this.awaitingRecent.delete(name);
		} else if (!launchActive(value)) this.awaitingRecent.delete(name);
	}
	private accept(value: ClientLaunch) { this.operations = [...this.operations.filter(v => v.id !== value.id), value].sort((a, b) => a.id - b.id); this.recordPlay(value); }
	async sync() {
		if (this.disposed) return;
		if (this.timer) clearTimeout(this.timer);
		const generation = this.generation;
		const sequence = ++this.readSequence;
		const controller = this.controller();
		try {
			const values = await listClientLaunches(controller.signal);
			if (this.disposed || sequence !== this.readSequence || generation !== this.generation) return;
			this.operations = values; this.uncertain = false; this.error = null;
			for (const value of values) this.recordPlay(value);
		} catch { if (!this.disposed && sequence === this.readSequence && generation === this.generation) { this.uncertain = true; this.error = '无法确认游戏运行状态，请刷新后重试。'; } }
		finally {
			this.controllers.delete(controller);
			if (!this.disposed && sequence === this.readSequence && (this.operations.some(launchActive) || generation !== this.generation)) this.timer = setTimeout(() => void this.sync(), 1500);
		}
	}
	async start(name: string, accountId: string | null, installJava = false, offlineTest = false) {
		if (this.disposed || this.uncertain || this.pending.includes(name) || launchActive(this.forClient(name))) return;
		this.pending = [...this.pending, name]; ++this.generation;
		this.awaitingRecent.add(name.toLowerCase());
		const controller = this.controller();
		try { const value = await startClientLaunch(name, accountId, installJava, controller.signal, offlineTest); if (!this.disposed) this.accept(value); }
		catch { if (!this.disposed) { this.uncertain = true; this.error = '启动请求未确认，请刷新状态后重试。'; } }
		finally { this.controllers.delete(controller); this.pending = this.pending.filter(v => v !== name); ++this.generation; if (!this.disposed) void this.sync(); }
	}
	async cancel(value: ClientLaunch) {
		const controller = this.controller(); ++this.generation;
		try { const result = await stopClientLaunch(value.id, controller.signal); if (!this.disposed) this.accept(result); }
		catch { if (!this.disposed) this.uncertain = true; }
		finally { this.controllers.delete(controller); ++this.generation; if (!this.disposed) void this.sync(); }
	}
	dispose() { this.disposed = true; if (this.timer) clearTimeout(this.timer); for (const controller of this.controllers) controller.abort(); }
}
export const [getClientLaunchState, setClientLaunchState] = createContext<ClientLaunchState>();
