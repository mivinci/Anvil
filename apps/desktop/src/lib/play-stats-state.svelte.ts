import { requestErrorMessage } from './runtime-errors.ts';
import { localeState } from './locale.svelte.ts';
import { clearPlayStats, getPlayStats, RuntimeRequestError, type PlayStatsClearTarget, type PlayStatsSnapshot } from './runtime';

/** On-demand reads; the Runtime records sessions, the UI never uploads timings. */
export class PlayStatsState {
	errorDetail = $state<string | null>(null);
	clearErrorDetail = $state<string | null>(null);
	snapshot = $state<PlayStatsSnapshot | null>(null);
	loading = $state(false);
	private errorValue = $state<string | (() => string | null) | null>(null);
	get error(): string | null { const value = this.errorValue; return typeof value === 'function' ? value() : value; }
	set error(value: string | (() => string | null) | null) { this.errorValue = value; if (value === null) this.errorDetail = null; }
	available = $state<boolean | null>(null);
	clearing = $state(false);
	private clearErrorValue = $state<string | (() => string | null) | null>(null);
	get clearError(): string | null { const value = this.clearErrorValue; return typeof value === 'function' ? value() : value; }
	set clearError(value: string | (() => string | null) | null) { this.clearErrorValue = value; if (value === null) this.clearErrorDetail = null; }
	clearUncertain = $state(false);
	private request: AbortController | null = null;
	private mutation: AbortController | null = null;
	private queued = false;
	private disposed = false;

	async refresh(): Promise<boolean> {
		if (this.disposed || this.clearing) return false;
		if (this.request) {
			this.queued = true;
			return false;
		}
		const controller = new AbortController();
		this.request = controller;
		this.loading = true;
		this.error = null;
		try {
			const snapshot = await getPlayStats(
				AbortSignal.any([controller.signal, AbortSignal.timeout(10_000)]),
			);
			if (this.request !== controller || controller.signal.aborted || this.disposed) return false;
			this.snapshot = snapshot;
			this.available = true;
			this.clearError = null;
			this.clearUncertain = false;
			return true;
		} catch (reason) {
			if (this.request !== controller || controller.signal.aborted || this.disposed) return false;
			// An older Runtime with the same API version may not serve this endpoint.
			if (reason instanceof RuntimeRequestError && reason.status === 404) {
				this.snapshot = { schemaVersion: 1, accounts: {} };
				this.available = false;
				return false;
			}
			this.errorDetail = reason instanceof Error ? reason.message : String(reason);
			this.error = () => requestErrorMessage(reason, localeState.current, localeState.messages.account.flow.statsReadFailed);
			return false;
		} finally {
			if (this.request === controller) {
				this.request = null;
				this.loading = false;
				if (this.queued) {
					this.queued = false;
					void this.refresh();
				}
			}
		}
	}

	async clear(target: PlayStatsClearTarget): Promise<boolean> {
		if (this.disposed || this.clearing || this.clearUncertain) return false;
		this.request?.abort();
		this.request = null;
		this.queued = false;
		this.loading = false;
		const controller = new AbortController();
		this.mutation = controller;
		this.clearing = true;
		this.clearError = null;
		try {
			const snapshot = await clearPlayStats(target, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (this.disposed) return false;
			this.snapshot = snapshot;
			this.available = true;
			this.error = null;
			return true;
		} catch (reason) {
			if (this.disposed) return false;
			this.clearUncertain = !(reason instanceof RuntimeRequestError && reason.status >= 400 && reason.status < 500);
			this.clearErrorDetail = reason instanceof Error ? reason.message : String(reason);
			const key = this.clearUncertain ? 'statsClearUnknown'
				: reason instanceof RuntimeRequestError && reason.status === 404 ? 'statsClearUnsupported' : 'statsClearFailed';
			this.clearError = () => key === 'statsClearUnknown' ? localeState.messages.account.flow[key] : requestErrorMessage(reason, localeState.current, localeState.messages.account.flow[key]);
			return false;
		} finally {
			if (this.mutation === controller) { this.mutation = null; this.clearing = false; }
		}
	}

	dispose() {
		this.disposed = true;
		this.queued = false;
		this.request?.abort();
		this.request = null;
		this.mutation?.abort();
		this.loading = false;
	}
}
