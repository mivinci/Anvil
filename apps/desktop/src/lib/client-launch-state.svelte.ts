import { runtimeErrorMessage } from './runtime-errors.ts';
import { localeState } from './locale.svelte.ts';
import { createContext } from 'svelte';
import { readRecentPlays, recordRecentPlay, forgetRecentPlay, clearRecentPlays } from './recent-plays';
import { listClientLaunches, startClientLaunch, stopClientLaunch, type ClientLaunch } from './runtime';
export const launchActive = (value: ClientLaunch | undefined) => !!value && !['exited', 'failed', 'cancelled'].includes(value.phase);
export const launchNeedsLogin = (value: ClientLaunch | undefined) =>
	value?.phase === 'failed' && !value.offlineTest &&
	(value.errorCode === 'account_required' || value.errorCode === 'authentication_failed');
export function launchLabel(value: ClientLaunch): string {
	if (value.cancelRequested) return localeState.messages.clients.flow.cancellingLaunch;
	const java = `Java${value.javaMajor ? ` ${value.javaMajor}` : ''}`;
	switch (value.phase) {
		case 'preparing': return localeState.messages.clients.flow.preparingLaunch;
		case 'authenticating': return localeState.messages.clients.flow.authenticating;
		case 'planning': return localeState.messages.clients.flow.planningLaunch;
		case 'java':
			return value.javaStage === 'completed' ? localeState.messages.clients.flow.startingGame : localeState.messages.clients.javaStage(value.javaStage, java);
		case 'running': return value.offlineTest ? localeState.messages.clients.flow.offlineRunning : localeState.messages.clients.flow.running;
		case 'stopping': return localeState.messages.clients.flow.stoppingGame;
		case 'failed': return runtimeErrorMessage(value.errorCode, localeState.current, localeState.messages.clients.flow.launchFailed);
		case 'cancelled': return localeState.messages.clients.flow.launchCancelled;
		case 'exited': return localeState.messages.clients.flow.exited;
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
	private errorValue = $state<string | (() => string) | null>(null);
	get error(): string | null { const value = this.errorValue; return typeof value === 'function' ? value() : value; }
	set error(value: string | (() => string) | null) { this.errorValue = value; }
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
		} catch { if (!this.disposed && sequence === this.readSequence && generation === this.generation) { this.uncertain = true; this.error = () => (localeState.messages.clients.flow.runningUnknown); } }
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
		catch { if (!this.disposed) { this.uncertain = true; this.error = () => (localeState.messages.clients.flow.launchUnknown); } }
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
