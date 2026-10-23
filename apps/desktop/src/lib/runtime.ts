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

export type ClientLoaderSpec = { kind: 'fabric'; version: string };
export interface InstalledClient {
	loader?: ClientLoaderSpec | null;
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
	loader: ClientLoaderSpec | null;
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
		loader: ClientLoaderSpec | null;
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

export const startClientInstall = (name: string, versionId: string, options: Pick<RequestInit, 'signal'> & { loader?: ClientLoaderSpec | null } = {}) =>
	runtimeRequest<ClientInstallOperation>('/clients/install', {
		signal: options.signal ?? null,
		method: 'POST',
		body: JSON.stringify({ name, versionId, loader: options.loader ?? null })
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

export interface ClientDetails extends InstalledClient {
	versionType: string;
	javaMajor: number;
	os: string;
	architecture: string;
	fileCount: number;
}
export const getClientDetails = (name: string, signal: AbortSignal) =>
	runtimeRequest<ClientDetails>(`/clients/${encodeURIComponent(name)}`, { signal });

export interface ClientResourceEntry {
	sha256: string;
	filename: string;
	size: number;
	enabled: boolean;
	source: 'local' | 'curseforge';
	origin?: { projectId: number; fileId: number; projectName: string; sha1: string; direct: boolean; requiredProjects: number[]; incompatibleProjects: number[] };
}
export interface ClientResourceManifest {
	schemaVersion: 1 | 2;
	revision: number;
	appliedRevision?: number | null;
	mods: ClientResourceEntry[];
}
export const listClientResources = (name: string, signal: AbortSignal) =>
	runtimeRequest<ClientResourceManifest>(`/clients/${encodeURIComponent(name)}/resources`, { signal });
export const importClientMod = (name: string, file: File, revision: number, signal: AbortSignal) =>
	runtimeRequest<ClientResourceManifest>(`/clients/${encodeURIComponent(name)}/resources?${new URLSearchParams({ filename: file.name, revision: String(revision) })}`, { method: 'POST', headers: { 'Content-Type': 'application/octet-stream' }, body: file, signal });
export const changeClientMod = (name: string, sha256: string, revision: number, enabled: boolean | null, signal: AbortSignal) =>
	runtimeRequest<ClientResourceManifest>(`/clients/${encodeURIComponent(name)}/resources/${encodeURIComponent(sha256)}${enabled === null ? `?revision=${revision}` : '/enabled'}`, enabled === null ? { method: 'DELETE', signal } : { method: 'POST', body: JSON.stringify({ revision, enabled }), signal });

export interface FabricLoaderVersion { version: string; stable: boolean }
export const listFabricVersions = (game: string, signal: AbortSignal) =>
	runtimeRequest<FabricLoaderVersion[]>(`/client/fabric/versions/${encodeURIComponent(game)}`, { signal });

export const syncClientResources = (name: string, revision: number, signal: AbortSignal) =>
	runtimeRequest<ClientResourceManifest>(`/clients/${encodeURIComponent(name)}/resources/sync`, { method: 'POST', body: JSON.stringify({ revision }), signal });

export interface CurseForgeConfiguration { configured: boolean; issue: string | null }
export interface CurseForgeProject {
	id: number; name: string; summary: string; authors: { name: string }[];
	links: { websiteUrl: string | null };
	allowModDistribution: boolean | null;
}
export interface CurseForgeFile {
	id: number; modId: number; displayName: string; fileName: string; fileLength: number;
	fileDate: string; releaseType: number; isAvailable: boolean;
}
export interface CurseForgePage<T> { items: T[]; nextIndex: number | null }
export interface CurseForgePreview {
	gameVersion: string; revision: number; totalBytes: number;
	items: { project: CurseForgeProject; file: CurseForgeFile; direct: boolean }[];
	optionalProjects: number[]; warnings: string[];
}
export const getCurseForgeConfiguration = (signal: AbortSignal) =>
	runtimeRequest<CurseForgeConfiguration>('/resources/curseforge/configuration', { signal });
export const searchCurseForgeProjects = (name: string, q: string, index: number, signal: AbortSignal) =>
	runtimeRequest<CurseForgePage<CurseForgeProject>>(`/clients/${encodeURIComponent(name)}/curseforge/projects?${new URLSearchParams({ q, index: String(index) })}`, { signal });
export const listCurseForgeFiles = (name: string, project: number, index: number, signal: AbortSignal) =>
	runtimeRequest<CurseForgePage<CurseForgeFile>>(`/clients/${encodeURIComponent(name)}/curseforge/projects/${project}/files?index=${index}`, { signal });
export const previewCurseForgeDependencies = (name: string, project: number, file: number, signal: AbortSignal) =>
	runtimeRequest<CurseForgePreview>(`/clients/${encodeURIComponent(name)}/curseforge/projects/${project}/files/${file}/preview`, { signal });


export interface CurseForgeInstallPlan extends CurseForgePreview {
	planId: number; expiresInMs: number; downloadBytes: number;
	files: { projectId: number; fileId: number; projectName: string; filename: string; size: number; direct: boolean; reuse: boolean }[];
}
export interface ResourceInstallOperation {
	id: number; name: string; state: 'running' | 'completed' | 'failed' | 'cancelled'; cancelRequested: boolean;
	progress: { stage: 'preparing' | 'downloading' | 'publishing' | 'completed'; completedFiles: number; totalFiles: number; downloadedBytes: number; totalBytes: number; receivedBytes: number };
	revision: number | null; error: string | null;
}
export const createCurseForgeInstallPlan = (name: string, project: number, file: number, signal: AbortSignal) =>
	runtimeRequest<CurseForgeInstallPlan>(`/clients/${encodeURIComponent(name)}/curseforge/projects/${project}/files/${file}/plan`, { method: 'POST', signal });
export const startResourceInstall = (name: string, plan: number, signal: AbortSignal) =>
	runtimeRequest<ResourceInstallOperation>(`/clients/${encodeURIComponent(name)}/resource-plans/${plan}/install`, { method: 'POST', signal });
export const listResourceOperations = (signal: AbortSignal) =>
	runtimeRequest<ResourceInstallOperation[]>('/resource-operations', { signal });
export const cancelResourceOperation = (id: number, signal: AbortSignal) =>
	runtimeRequest<ResourceInstallOperation>(`/resource-operations/${id}`, { method: 'DELETE', signal });
