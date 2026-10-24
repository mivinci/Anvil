import { createContext } from 'svelte';
import { listInstalledClients, type InstalledClient, type ClientInstallOperation } from './runtime';

type InstallReceipt = NonNullable<ClientInstallOperation['result']>;

/** Window-owned installed records. Failed refreshes never erase confirmed installs. */
export class ClientLibraryState {
	clients = $state<InstalledClient[]>([]);
	loaded = $state(false);
	loading = $state(false);
	error = $state<string | null>(null);
	query = $state('');
	lastCompleted = $state<InstallReceipt | null>(null);
	removalNotice = $state<string | null>(null);
	acceptRemoved(name: string, cleanupPending: boolean) {
		this.revision++;
		this.clients = this.clients.filter(client => client.name.toLowerCase() !== name.toLowerCase());
		if (this.lastCompleted?.name.toLowerCase() === name.toLowerCase()) this.lastCompleted = null;
		this.removalNotice = cleanupPending ? `实例 ${name} 已移除，部分文件可能尚未清理，请查看日志。` : `已删除实例 ${name}。`;
		void this.refresh();
	}
	private disposed = false;
	private controller: AbortController | null = null;
	private refreshQueued = false;
	private revision = 0;

	acceptCompleted(receipt: InstallReceipt) {
		if (this.disposed) return;
		this.revision++;
		this.lastCompleted = receipt;
		this.clients = [...this.clients.filter((client) => client.id !== receipt.id), {
			id: receipt.id, name: receipt.name, versionId: receipt.versionId, loader: receipt.loader,
		}].sort((a, b) => a.id.localeCompare(b.id));
		this.loaded = true;
		void this.refresh();
	}

	async refresh() {
		if (this.disposed) return;
		if (this.controller) { this.refreshQueued = true; return; }
		const controller = new AbortController();
		const revision = this.revision;
		this.controller = controller;
		this.loading = true;
		this.error = null;
		try {
			const result = await listInstalledClients({ signal: AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]) });
			if (this.disposed) return;
			if (!Array.isArray(result) || result.some((item) => !item || typeof item.id !== 'string' || typeof item.name !== 'string' || typeof item.versionId !== 'string') || new Set(result.map((item) => item.id)).size !== result.length) throw new Error('客户端列表格式不正确。');
			// A read started before publication must not overwrite the newer receipt.
			if (revision !== this.revision) { this.refreshQueued = true; return; }
			this.clients = result;
			this.loaded = true;
		} catch (reason) {
			if (!this.disposed) this.error = reason instanceof Error ? reason.message : String(reason);
		} finally {
			if (this.controller === controller) this.controller = null;
			if (!this.disposed) {
				this.loading = false;
				if (this.refreshQueued) { this.refreshQueued = false; void this.refresh(); }
			}
		}
	}

	dispose() { this.disposed = true; this.refreshQueued = false; this.controller?.abort(); }
}
export const [getClientLibraryState, setClientLibraryState] = createContext<ClientLibraryState>();
