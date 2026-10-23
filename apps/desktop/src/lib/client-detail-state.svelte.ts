import { createContext } from 'svelte';
import { getClientDetails, RuntimeRequestError, type ClientDetails } from './runtime';

export class ClientDetailState {
	client = $state<ClientDetails | null>(null);
	loading = $state(false);
	error = $state<string | null>(null);
	notFound = $state(false);
	private request: AbortController | null = null;
	private name = '';

	async load(name: string) {
		name = name.toLowerCase();
		this.request?.abort();
		const controller = new AbortController();
		this.request = controller;
		if (name !== this.name) this.client = null;
		this.name = name;
		this.loading = true;
		this.error = null;
		this.notFound = false;
		try {
			const client = await getClientDetails(name, AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]));
			if (controller.signal.aborted) return;
			if (!client || typeof client.id !== 'string' || typeof client.name !== 'string' || typeof client.versionId !== 'string' || typeof client.versionType !== 'string' || !Number.isInteger(client.javaMajor) || typeof client.os !== 'string' || typeof client.architecture !== 'string' || !Number.isInteger(client.fileCount)) throw new Error('实例详情格式不正确。');
			this.client = client;
		} catch (error) {
			if (controller.signal.aborted) return;
			this.notFound = error instanceof RuntimeRequestError && error.status === 404;
			if (this.notFound) this.client = null;
			this.error = this.notFound ? '未找到这个实例。' : '暂时无法读取实例详情，请稍后重试。';
		} finally {
			if (this.request === controller) { this.request = null; this.loading = false; }
		}
	}
	cancel() { this.request?.abort(); this.request = null; this.loading = false; }
}
export const [getClientDetailState, setClientDetailState] = createContext<ClientDetailState>();
