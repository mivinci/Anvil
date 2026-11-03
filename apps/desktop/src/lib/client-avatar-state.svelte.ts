import { createContext } from 'svelte';
import { getClientAvatar, saveClientAvatar, clearClientAvatar } from './runtime.ts';

export interface AvatarView {
	url: string | null;
	revision: string | null;
	custom: boolean;
	loaded: boolean;
	loading: boolean;
	error: boolean;
}
const empty: AvatarView = { url: null, revision: null, custom: false, loaded: false, loading: false, error: false };

/** Window-owned avatar blobs. Never stores tokens or remote URLs. */
export class ClientAvatarState {
	private views = $state<Record<string, AvatarView>>({});
	private controllers = new Map<string, AbortController>();
	private disposed = false;

	view(name: string): AvatarView {
		return this.views[name.toLowerCase()] ?? empty;
	}

	async ensure(name: string): Promise<void> {
		const key = name.toLowerCase();
		const current = this.view(key);
		if (this.disposed || current.loading || current.loaded) return;
		const controller = new AbortController();
		this.controllers.set(key, controller);
		this.views = { ...this.views, [key]: { ...current, loading: true, error: false } };
		try {
			const image = await getClientAvatar(name, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (this.disposed || this.controllers.get(key) !== controller) return;
			if (!image) {
				this.views = { ...this.views, [key]: { url: null, revision: null, custom: false, loaded: true, loading: false, error: false } };
				return;
			}
			const previous = this.view(key).url;
			if (previous) URL.revokeObjectURL(previous);
			this.views = { ...this.views, [key]: { url: URL.createObjectURL(image.blob), revision: image.revision, custom: true, loaded: true, loading: false, error: false } };
		} catch {
			if (!this.disposed && this.controllers.get(key) === controller) {
				this.views = { ...this.views, [key]: { ...this.view(key), loading: false, loaded: true, error: true } };
			}
		} finally {
			if (this.controllers.get(key) === controller) this.controllers.delete(key);
		}
	}

	async upload(name: string, blob: Blob): Promise<void> {
		const key = name.toLowerCase();
		this.controllers.get(key)?.abort();
		this.controllers.delete(key);
		let change;
		try {
			change = await saveClientAvatar(name, blob, AbortSignal.timeout(30_000));
		} catch (error) {
			if (!this.disposed) this.views = { ...this.views, [key]: { ...this.view(key), loading: false } };
			throw error;
		}
		if (this.disposed) return;
		const previous = this.view(key).url;
		if (previous) URL.revokeObjectURL(previous);
		this.views = { ...this.views, [key]: { url: URL.createObjectURL(blob), revision: change.revision, custom: true, loaded: true, loading: false, error: false } };
	}

	async clear(name: string): Promise<void> {
		const key = name.toLowerCase();
		this.controllers.get(key)?.abort();
		this.controllers.delete(key);
		try {
			await clearClientAvatar(name, AbortSignal.timeout(30_000));
		} catch (error) {
			if (!this.disposed) this.views = { ...this.views, [key]: { ...this.view(key), loading: false } };
			throw error;
		}
		if (this.disposed) return;
		const previous = this.view(key).url;
		if (previous) URL.revokeObjectURL(previous);
		this.views = { ...this.views, [key]: { url: null, revision: null, custom: false, loaded: true, loading: false, error: false } };
	}

	forget(name: string): void {
		const key = name.toLowerCase();
		this.controllers.get(key)?.abort();
		this.controllers.delete(key);
		const view = this.views[key];
		if (view?.url) URL.revokeObjectURL(view.url);
		const { [key]: _removed, ...remaining } = this.views;
		this.views = remaining;
	}

	dispose(): void {
		this.disposed = true;
		for (const controller of this.controllers.values()) controller.abort();
		this.controllers.clear();
		for (const view of Object.values(this.views)) if (view.url) URL.revokeObjectURL(view.url);
		this.views = {};
	}
}
export const [getClientAvatarState, setClientAvatarState] = createContext<ClientAvatarState>();
