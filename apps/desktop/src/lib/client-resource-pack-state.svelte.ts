import { localPackFileError, localPackSize } from './local-pack';
import {
	listClientResourcePacks, importClientResourcePack, removeClientResourcePack, syncClientResources,
	type ClientResourcePacks, type ClientResourcePack, type ResourcePackFormat,
} from './runtime';

const MAX_BYTES = 64 * 1024 * 1024;
const MAX_FORMAT = 2_147_483_647;
function validFormat(value: ResourcePackFormat) {
	return value && [value.major, value.minor].every(part => Number.isInteger(part) && part >= 0 && part <= MAX_FORMAT);
}
export function validateClientResourcePacks(result: ClientResourcePacks): ClientResourcePacks {
	if (!result || !Number.isSafeInteger(result.revision) || result.revision < 0
		|| !(result.appliedRevision === null || (Number.isSafeInteger(result.appliedRevision) && result.appliedRevision >= 0 && result.appliedRevision <= result.revision))
		|| !Array.isArray(result.packs) || result.packs.length > 1024) throw new Error('资源包清单格式不正确。');
	const hashes = new Set<string>(), names = new Set<string>();
	for (const pack of result.packs) {
		const meta = pack?.metadata;
		if (!pack || pack.source !== 'local' || typeof pack.filename !== 'string' || pack.filename.length > 128
			|| !/^[a-f0-9]{64}$/.test(pack.sha256) || !Number.isSafeInteger(pack.size) || pack.size < 1 || pack.size > MAX_BYTES
			|| !meta || typeof meta.description !== 'string' || Array.from(meta.description).length > 1024
			|| !validFormat(meta.minFormat) || !validFormat(meta.maxFormat)
			|| meta.minFormat.major > meta.maxFormat.major || (meta.minFormat.major === meta.maxFormat.major && meta.minFormat.minor > meta.maxFormat.minor)
			|| !(meta.packFormat === null || (Number.isInteger(meta.packFormat) && meta.packFormat >= meta.minFormat.major && meta.packFormat <= meta.maxFormat.major))
			|| hashes.has(pack.sha256) || names.has(pack.filename.toLowerCase())) throw new Error('资源包条目格式不正确。');
		hashes.add(pack.sha256); names.add(pack.filename.toLowerCase());
	}
	return result;
}
export const resourcePackSize = localPackSize;
export function resourcePackFormat(pack: ClientResourcePack) {
	const { minFormat: min, maxFormat: max } = pack.metadata;
	if (min.minor === 0 && max.minor === MAX_FORMAT) return min.major === max.major ? `${min.major}` : `${min.major}–${max.major}`;
	const format = (value: ResourcePackFormat) => `${value.major}.${value.minor === MAX_FORMAT ? '*' : value.minor}`;
	return min.major === max.major && min.minor === max.minor ? format(min) : `${format(min)}–${format(max)}`;
}

/** Page-local reads and explicit mutations; no polling and no automatic mutation retries. */
export class ClientResourcePackState {
	collection = $state<ClientResourcePacks | null>(null);
	activity = $state<'loading' | 'importing' | 'removing' | 'syncing' | null>(null);
	error = $state<string | null>(null);
	uncertain = $state(false);
	selected = $state<File | null>(null);
	removing = $state<string | null>(null);
	message = $state<string | null>(null);
	readonly busy = $derived(this.activity !== null);
	private name = '';
	private request: AbortController | null = null;

	open(name: string) {
		this.dispose(); this.name = name;
		this.collection = null; this.selected = null; this.removing = null;
		this.error = null; this.uncertain = false; this.message = null;
		void this.refresh();
	}
	dispose() { this.request?.abort(); this.request = null; this.activity = null; }
	async refresh() {
		const name = this.name;
		await this.run(signal => listClientResourcePacks(name, signal), 'loading');
	}
	pick(file: File) {
		if (this.busy || this.uncertain || !this.collection) return;
		this.selected = null; this.removing = null; this.message = null;
		const error = localPackFileError(file);
		if (error) { this.error = error; return; }
		this.selected = file; this.error = null;
	}
	async importSelected() {
		if (!this.collection || !this.selected || this.busy || this.uncertain) return;
		const name = this.name, file = this.selected, revision = this.collection.revision;
		await this.run(signal => importClientResourcePack(name, file, revision, signal), 'importing');
	}
	async remove(pack: ClientResourcePack) {
		if (!this.collection || this.busy || this.uncertain || this.removing !== pack.sha256) return;
		const name = this.name, revision = this.collection.revision;
		await this.run(signal => removeClientResourcePack(name, pack.sha256, revision, signal), 'removing');
	}
	async sync() {
		if (!this.collection || this.busy || this.uncertain) return;
		const name = this.name, revision = this.collection.revision;
		await this.run(async signal => {
			const result = await syncClientResources(name, revision, signal);
			return { revision: result.revision, appliedRevision: result.appliedRevision ?? null, packs: result.resourcePacks ?? [] };
		}, 'syncing');
	}
	private async run(action: (signal: AbortSignal) => Promise<ClientResourcePacks>, activity: NonNullable<ClientResourcePackState['activity']>) {
		if (this.request) return;
		const controller = new AbortController(); this.request = controller;
		const mutation = activity !== 'loading';
		this.activity = activity; this.error = null; this.message = null;
		try {
			const result = validateClientResourcePacks(await action(AbortSignal.any([controller.signal, AbortSignal.timeout(65_000)])));
			if (this.request !== controller || controller.signal.aborted) return;
			this.collection = result;
			if (mutation || this.uncertain) { this.selected = null; this.removing = null; }
			this.uncertain = false;
			this.message = activity === 'importing' ? '已添加到实例，请在游戏内选择资源包。' : activity === 'removing' ? '已从此实例移除，共享原件保留。' : activity === 'syncing' ? '受管资源文件已同步。' : null;
		} catch (reason) {
			if (this.request !== controller || controller.signal.aborted) return;
			if (mutation) this.uncertain = true;
			const detail = reason instanceof Error ? reason.message : '';
			this.error = mutation ? `操作结果未确认，请刷新清单后再操作。${detail}` : `暂时无法读取资源包清单。${detail}`;
		} finally {
			if (this.request === controller) { this.request = null; this.activity = null; }
		}
	}
}
