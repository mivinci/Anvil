import { shell } from './shell';

export interface RuntimeHealth {
	status: 'ok';
	fileLoggingAvailable?: boolean | null;
	apiVersion: string;
}

export interface GameVersion {
	id: string;
	type: 'release' | 'snapshot' | 'old_beta' | 'old_alpha' | string;
	releaseTime: string;
}

export interface InstalledClient {
	id: string;
	name: string;
	versionId: string;
}

export interface AccountSummary {
	id: string;
	username: string;
	selected: boolean;
}

export interface AccountLoginConfiguration {
	enabled: boolean;
	unavailableReason: 'invalid_client_id' | null;
}

export class RuntimeRequestError extends Error {
	constructor(message: string, readonly status: number) {
		super(message);
		this.name = 'RuntimeRequestError';
	}
}

export type AccountLoginOperation = {
	id: number;
	state: 'running' | 'completed' | 'failed' | 'cancelled';
	stage: 'requesting_code' | 'waiting_for_approval' | 'checking_profile' | 'saving_account';
	cancelRequested: boolean;
	authorization:
		| {
				kind: 'device_code';
				verificationUri: string;
				userCode: string;
				expiresInSeconds: number;
				pollIntervalSeconds: number;
				/** Unix timestamp in seconds, not milliseconds. */
				expiresAt: number;
		  }
		| null;
	account: AccountSummary | null;
	error: string | null;
};

export type ClientInstallStage =
	| 'resolving'
	| 'planning'
	| 'downloading'
	| 'verifying'
	| 'publishing'
	| 'completed';

export interface ClientInstallOperation {
	id: number;
	name: string;
	versionId: string;
	cancelRequested: boolean;
	state: 'running' | 'pausing' | 'paused' | 'resuming' | 'completed' | 'failed' | 'cancelled';
	progress: {
		stage: ClientInstallStage;
		completedFiles: number;
		totalFiles: number | null;
		downloadedBytes: number;
		totalBytes: number | null;
		receivedBytes: number;
	};
	result: {
		id: string;
		name: string;
		versionId: string;
		alreadyInstalled: boolean;
	} | null;
	error: string | null;
}

async function getRuntimeConnection() {
	const connection = await shell.getRuntimeConnection();
	const baseUrl = new URL(connection.baseUrl);
	if (baseUrl.protocol !== 'http:' || baseUrl.hostname !== '127.0.0.1') {
		throw new Error('Shell returned an invalid Runtime address');
	}
	return { connection, baseUrl };
}

async function runtimeRequest<T>(path: string, init: RequestInit = {}): Promise<T> {
	const { connection, baseUrl } = await getRuntimeConnection();
	const headers = new Headers(init.headers);
	headers.set('Authorization', `Bearer ${connection.token}`);
	if (init.body && !headers.has('Content-Type')) headers.set('Content-Type', 'application/json');

	init.signal?.throwIfAborted();
	const response = await fetch(new URL(path, baseUrl), { ...init, headers });
	const payload: unknown = await response.json().catch(() => null);
	if (!response.ok) {
		const message =
			typeof payload === 'object' && payload !== null && 'error' in payload &&
			typeof payload.error === 'string'
				? payload.error
				: `Runtime request failed (${response.status})`;
		throw new RuntimeRequestError(message, response.status);
	}
	if (payload === null) throw new Error('Runtime returned an invalid JSON response');
	return payload as T;
}

/** Request the health endpoint directly from the authorized local sidecar. */
export async function getRuntimeHealth(): Promise<RuntimeHealth> {
	try {
		const { connection, baseUrl } = await getRuntimeConnection();

		const response = await fetch(new URL('/health', baseUrl), {
			headers: { Authorization: `Bearer ${connection.token}` }
		});
		if (!response.ok) throw new Error(`Runtime health request failed (${response.status})`);

		const health = (await response.json()) as RuntimeHealth;
		if (health.status !== 'ok' || health.apiVersion !== connection.apiVersion) {
			throw new Error('Runtime API version does not match the Shell connection');
		}
		await shell.confirmAppReady();
		return health;
	} catch (error) {
		await shell.reportAppStartupFailure().catch(() => false);
		throw error;
	}
}

/** Read diagnostics without changing activation health or triggering rollback. */
export async function getRuntimeFileLoggingStatus(): Promise<boolean | null> {
	const health = await runtimeRequest<RuntimeHealth>('/health', { signal: AbortSignal.timeout(5000) });
	return typeof health.fileLoggingAvailable === 'boolean' ? health.fileLoggingAvailable : null;
}

export interface ClientVersionCatalog {
	versions: GameVersion[];
	stale: boolean;
	freshForMs: number;
	revision: number;
	refreshError: string | null;
}
export const listClientVersionCatalog = (options: { signal?: AbortSignal; refresh?: boolean; after?: number } = {}) =>
	runtimeRequest<ClientVersionCatalog>(`/client/version-catalog${options.refresh ? `?refresh=true${options.after !== undefined ? `&after=${options.after}` : ''}` : ''}`, { signal: options.signal ?? null });

export const listInstalledClients = (options: { signal?: AbortSignal } = {}) =>
	runtimeRequest<InstalledClient[]>('/clients', { signal: options.signal ?? null });

export const listClientInstallOperations = (options: Pick<RequestInit, 'signal'> = {}) =>
	runtimeRequest<ClientInstallOperation[]>('/operations', options);

export const startClientInstall = (name: string, versionId: string, options: Pick<RequestInit, 'signal'> = {}) =>
	runtimeRequest<ClientInstallOperation>('/clients/install', {
		...options,
		method: 'POST',
		body: JSON.stringify({ name, versionId })
	});

export const getClientInstallOperation = (id: number, options: Pick<RequestInit, 'signal'> = {}) =>
	runtimeRequest<ClientInstallOperation>(`/operations/${id}`, options);

export const cancelClientInstall = (id: number, options: Pick<RequestInit, 'signal'> = {}) =>
	runtimeRequest<ClientInstallOperation>(`/operations/${id}`, { ...options, method: 'DELETE' });

export const pauseClientInstall = (id: number, options: Pick<RequestInit, 'signal'> = {}) =>
	runtimeRequest<ClientInstallOperation>(`/operations/${id}/pause`, { ...options, method: 'POST' });

export const resumeClientInstall = (id: number, options: Pick<RequestInit, 'signal'> = {}) =>
	runtimeRequest<ClientInstallOperation>(`/operations/${id}/resume`, { ...options, method: 'POST' });

export type AccountRequestOptions = Pick<RequestInit, 'signal'>;

export const listAccounts = (options: AccountRequestOptions = {}) =>
	runtimeRequest<AccountSummary[]>('/accounts', options);

export const getAccountLoginConfiguration = (options: AccountRequestOptions = {}) =>
	runtimeRequest<AccountLoginConfiguration>('/accounts/login', options);

export const startAccountLogin = () =>
	runtimeRequest<AccountLoginOperation>('/accounts/login', {
		method: 'POST'
	});

export const getAccountLoginOperation = (id: number, options: AccountRequestOptions = {}) =>
	runtimeRequest<AccountLoginOperation>(`/account-operations/${id}`, options);

export const getActiveAccountLogin = (options: AccountRequestOptions = {}) =>
	runtimeRequest<{ operation: AccountLoginOperation | null }>('/account-operations/active', options);

export const cancelAccountLogin = (id: number, options: AccountRequestOptions = {}) =>
	runtimeRequest<AccountLoginOperation>(`/account-operations/${id}`, { method: 'DELETE', ...options });

export const selectAccount = (id: string, options: AccountRequestOptions = {}) =>
	runtimeRequest<AccountSummary[]>(`/accounts/${encodeURIComponent(id)}/select`, { method: 'POST', ...options });

export const logoutAccount = (id: string, options: AccountRequestOptions = {}) =>
	runtimeRequest<AccountSummary[]>(`/accounts/${encodeURIComponent(id)}`, { method: 'DELETE', ...options });
