import { getPlayStats, RuntimeRequestError, type PlayStatsSnapshot } from './runtime';

/** On-demand reads; the Runtime records sessions, the UI never uploads timings. */
export class PlayStatsState {
	snapshot = $state<PlayStatsSnapshot | null>(null);
	loading = $state(false);
	error = $state<string | null>(null);
	private request: AbortController | null = null;
	private queued = false;

	async refresh() {
		if (this.request) {
			this.queued = true;
			return;
		}
		const controller = new AbortController();
		this.request = controller;
		this.loading = true;
		this.error = null;
		try {
			const snapshot = await getPlayStats(
				AbortSignal.any([controller.signal, AbortSignal.timeout(10_000)]),
			);
			if (this.request !== controller || controller.signal.aborted) return;
			this.snapshot = snapshot;
		} catch (reason) {
			if (this.request !== controller || controller.signal.aborted) return;
			// An older Runtime with the same API version may not serve this endpoint.
			if (reason instanceof RuntimeRequestError && reason.status === 404) {
				this.snapshot = { schemaVersion: 1, accounts: {} };
				return;
			}
			this.error = reason instanceof Error ? reason.message : '暂时无法读取游玩统计。';
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

	dispose() {
		this.queued = false;
		this.request?.abort();
		this.request = null;
		this.loading = false;
	}
}
