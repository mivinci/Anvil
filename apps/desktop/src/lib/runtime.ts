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

export interface AccountSkin { url: string; model: 'classic' | 'slim' }
export const getAccountSkin = (id: string, signal: AbortSignal) =>
	runtimeRequest<AccountSkin | null>(`/accounts/${encodeURIComponent(id)}/skin`, { signal });

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
	| 'java'
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
		javaMajor: number | null;
		javaStage: ClientLaunch['javaStage'];
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
		downloadedFiles?: number;
		reusedFiles?: number;
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
export async function getRuntimeHealth(signal: AbortSignal = AbortSignal.timeout(5000)): Promise<RuntimeHealth> {
	try {
		const { connection, baseUrl } = await getRuntimeConnection();

		const response = await fetch(new URL('/health', baseUrl), {
			signal,
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

export interface MinecraftNewsItem {
	title: string;
	url: string;
	publishedAt: string | null;
	categories: string[];
}

export interface MinecraftNewsCatalog {
	items: MinecraftNewsItem[];
	stale: boolean;
	freshForMs: number;
	revision: number;
	refreshError: string | null;
}

export const getMinecraftNews = (options: { signal?: AbortSignal; refresh?: boolean } = {}) =>
	runtimeRequest<MinecraftNewsCatalog>(`/news${options.refresh ? '?refresh=true' : ''}`, { signal: options.signal ?? null });

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
export interface ClientRemoval { id: string; removed: boolean; cleanupPending: boolean }
export const deleteClient = (name: string, signal: AbortSignal) =>
	runtimeRequest<ClientRemoval>(`/clients/${encodeURIComponent(name)}`, { method: 'DELETE', body: JSON.stringify({ confirmation: name }), signal });

export interface ClientResourceEntry {
	sha256: string;
	filename: string;
	size: number;
	enabled: boolean;
	source: 'local' | 'curseforge' | 'modrinth';
	origin?: { projectId: number; fileId: number; projectName: string; sha1: string; direct: boolean; requiredProjects: number[]; incompatibleProjects: number[] };
	modrinthOrigin?: { projectId: string; versionId: string; projectName: string; sha1: string; direct: boolean; requiredVersions: Record<string, string>; incompatibleVersions: Record<string, string | null> };
}
export interface ClientResourceManifest {
	schemaVersion: 1 | 2 | 3 | 4 | 5;
	revision: number;
	appliedRevision?: number | null;
	mods: ClientResourceEntry[];
	resourcePacks?: ClientResourcePack[];
	shaderPacks?: ClientShaderPack[];
}
export const listClientResources = (name: string, signal: AbortSignal) =>
	runtimeRequest<ClientResourceManifest>(`/clients/${encodeURIComponent(name)}/resources`, { signal, cache: 'no-store' });
export const importClientMod = (name: string, file: File, revision: number, signal: AbortSignal) =>
	runtimeRequest<ClientResourceManifest>(`/clients/${encodeURIComponent(name)}/resources?${new URLSearchParams({ filename: file.name, revision: String(revision) })}`, { method: 'POST', headers: { 'Content-Type': 'application/octet-stream' }, body: file, signal });
export const changeClientMod = (name: string, sha256: string, revision: number, enabled: boolean | null, signal: AbortSignal) =>
	runtimeRequest<ClientResourceManifest>(`/clients/${encodeURIComponent(name)}/resources/${encodeURIComponent(sha256)}${enabled === null ? `?revision=${revision}` : '/enabled'}`, enabled === null ? { method: 'DELETE', signal } : { method: 'POST', body: JSON.stringify({ revision, enabled }), signal });

export interface ClientShaderPack {
	sha256: string; filename: string; size: number; source: 'local';
}
export interface ShaderMod { version: string; enabled: boolean }
export interface ShaderEnvironment {
	status: 'requires_fabric' | 'missing_iris' | 'iris_disabled' | 'missing_dependencies' | 'unconfirmed' | 'ready';
	iris: ShaderMod | null; sodium: ShaderMod | null; managedOnly: boolean; reason: string;
}
export interface ClientShaderPackChange {
	revision: number; appliedRevision: number | null; packs: ClientShaderPack[];
}
export interface ClientShaderPacks extends ClientShaderPackChange { environment: ShaderEnvironment }
export const listClientShaderPacks = (name: string, signal: AbortSignal) =>
	runtimeRequest<ClientShaderPacks>(`/clients/${encodeURIComponent(name)}/shader-packs`, { signal, cache: 'no-store' });
export const importClientShaderPack = (name: string, file: File, revision: number, signal: AbortSignal) =>
	runtimeRequest<ClientShaderPackChange>(`/clients/${encodeURIComponent(name)}/shader-packs?${new URLSearchParams({ filename: file.name, revision: String(revision) })}`, { method: 'POST', headers: { 'Content-Type': 'application/octet-stream' }, body: file, signal });
export const removeClientShaderPack = (name: string, sha256: string, revision: number, signal: AbortSignal) =>
	runtimeRequest<ClientShaderPackChange>(`/clients/${encodeURIComponent(name)}/shader-packs/${encodeURIComponent(sha256)}?revision=${revision}`, { method: 'DELETE', signal });

export interface ResourcePackFormat { major: number; minor: number }
export interface ClientResourcePack {
	sha256: string; filename: string; size: number; source: 'local';
	metadata: { description: string; packFormat: number | null; minFormat: ResourcePackFormat; maxFormat: ResourcePackFormat };
}
export interface ClientResourcePacks { revision: number; appliedRevision: number | null; packs: ClientResourcePack[] }
export const listClientResourcePacks = (name: string, signal: AbortSignal) =>
	runtimeRequest<ClientResourcePacks>(`/clients/${encodeURIComponent(name)}/resource-packs`, { signal });
export const importClientResourcePack = (name: string, file: File, revision: number, signal: AbortSignal) =>
	runtimeRequest<ClientResourcePacks>(`/clients/${encodeURIComponent(name)}/resource-packs?${new URLSearchParams({ filename: file.name, revision: String(revision) })}`, { method: 'POST', headers: { 'Content-Type': 'application/octet-stream' }, body: file, signal });
export const removeClientResourcePack = (name: string, sha256: string, revision: number, signal: AbortSignal) =>
	runtimeRequest<ClientResourcePacks>(`/clients/${encodeURIComponent(name)}/resource-packs/${encodeURIComponent(sha256)}?revision=${revision}`, { method: 'DELETE', signal });

export interface CatalogStatus { stale: boolean; freshForMs: number; revision: number; refreshError: string | null }
export interface CatalogOptions { refresh?: boolean; after?: number }
function catalogQuery(options: CatalogOptions = {}) {
	return { ...(options.refresh ? { refresh: 'true' } : {}), ...(options.after !== undefined ? { after: String(options.after) } : {}) };
}
export interface FabricLoaderVersion { version: string; stable: boolean }
export interface FabricVersionCatalog extends CatalogStatus { versions: FabricLoaderVersion[] }
export const listFabricVersions = (game: string, signal: AbortSignal, options?: CatalogOptions) =>
	runtimeRequest<FabricVersionCatalog>(`/client/fabric/versions/${encodeURIComponent(game)}?${new URLSearchParams(catalogQuery(options))}`, { signal });

export const syncClientResources = (name: string, revision: number, signal: AbortSignal) =>
	runtimeRequest<ClientResourceManifest>(`/clients/${encodeURIComponent(name)}/resources/sync`, { method: 'POST', body: JSON.stringify({ revision }), signal });

export interface CurseForgeConfiguration { configured: boolean; issue: string | null }
export interface CurseForgeProject {
	id: number; name: string; summary: string; authors: { name: string }[];
	links: { websiteUrl: string | null };
	allowModDistribution: boolean | null;
	logo?: { thumbnailUrl: string | null; url: string | null } | null;
	downloadCount?: number | null;
	dateModified?: string | null;
}
export interface CurseForgeFile {
	id: number; modId: number; displayName: string; fileName: string; fileLength: number;
	fileDate: string; releaseType: number; isAvailable: boolean;
}
export interface CurseForgePage<T> extends CatalogStatus { items: T[]; nextIndex: number | null }
export interface CurseForgePreview {
	gameVersion: string; revision: number; totalBytes: number;
	items: { project: CurseForgeProject; file: CurseForgeFile; direct: boolean }[];
	optionalProjects: number[]; warnings: string[];
}
export const getCurseForgeConfiguration = (signal: AbortSignal) =>
	runtimeRequest<CurseForgeConfiguration>('/resources/curseforge/configuration', { signal });
export const listCurseForgeFiles = (name: string, project: number, index: number, signal: AbortSignal, options?: CatalogOptions) =>
	runtimeRequest<CurseForgePage<CurseForgeFile>>(`/clients/${encodeURIComponent(name)}/curseforge/projects/${project}/files?${new URLSearchParams({ index: String(index), ...catalogQuery(options) })}`, { signal });


export interface CurseForgeInstallPlan extends CurseForgePreview {
	planId: number; expiresInMs: number; downloadBytes: number;
	files: { projectId: number; fileId: number; projectName: string; filename: string; size: number; direct: boolean; reuse: boolean }[];
}
export type ModSource = 'modrinth' | 'curseforge';
export type ModProjectSummary = Pick<CurseForgeProject, 'name' | 'summary' | 'authors' | 'links' | 'logo' | 'downloadCount' | 'dateModified'> & {
	id: string | number;
	iconUrl?: string | null;
	updatedAt?: string | null;
	allowModDistribution?: boolean | null;
};
export interface ModSearchOptions extends CatalogOptions { gameVersion?: string; loader?: 'fabric' }
export interface ModProjectDetails extends CatalogStatus { project: ModProjectSummary }
export const searchModProjects = (source: ModSource, q: string, index: number, signal: AbortSignal, options: ModSearchOptions = {}) =>
	runtimeRequest<CurseForgePage<ModProjectSummary>>(`/resources/${source}/projects?${new URLSearchParams({
		q, index: String(index), ...catalogQuery(options),
		...(options.gameVersion !== undefined ? { gameVersion: options.gameVersion } : {}),
		...(options.loader !== undefined ? { loader: options.loader } : {}),
	})}`, { signal });
export const getModProject = (source: ModSource, project: string | number, signal: AbortSignal, options?: CatalogOptions) =>
	runtimeRequest<ModProjectDetails>(`/resources/${source}/projects/${encodeURIComponent(project)}?${new URLSearchParams(catalogQuery(options))}`, { signal });
export type ModrinthProject = Omit<CurseForgeProject, 'id' | 'allowModDistribution' | 'logo' | 'dateModified'> & { id: string; iconUrl?: string | null; updatedAt?: string | null };
export type ModrinthFile = Omit<CurseForgeFile, 'id' | 'modId' | 'isAvailable'> & { id: string; modId: string; sha1: string };
export interface ModrinthInstallPlan extends Omit<CurseForgeInstallPlan, 'items' | 'files' | 'optionalProjects'> {
	items: { project: ModrinthProject; file: ModrinthFile; direct: boolean }[];
	files: { projectId: string; fileId: string; projectName: string; filename: string; size: number; direct: boolean; reuse: boolean }[];
	optionalProjects: string[];
}
export interface ShaderProjectPage extends CatalogStatus { items: ModProjectSummary[]; nextIndex: number | null }
export interface ShaderProjectDetails extends CatalogStatus { project: ModProjectSummary }
export interface ShaderVersionPage extends CatalogStatus { items: ModrinthFile[]; nextIndex: null }
export const searchShaderProjects = (q: string, index: number, gameVersion: string, signal: AbortSignal, options: CatalogOptions = {}) =>
	runtimeRequest<ShaderProjectPage>(`/resources/modrinth/shader-projects?${new URLSearchParams({
		q, index: String(index), gameVersion, ...catalogQuery(options),
	})}`, { signal });
export const getShaderProject = (project: string, signal: AbortSignal, options?: CatalogOptions) =>
	runtimeRequest<ShaderProjectDetails>(`/resources/modrinth/shader-projects/${encodeURIComponent(project)}?${new URLSearchParams(catalogQuery(options))}`, { signal });
export const listShaderVersions = (project: string, gameVersion: string, signal: AbortSignal, options?: CatalogOptions) =>
	runtimeRequest<ShaderVersionPage>(`/resources/modrinth/shader-projects/${encodeURIComponent(project)}/versions?${new URLSearchParams({ gameVersion, ...catalogQuery(options) })}`, { signal });
export interface ShaderInstallPlan {
	planId: number; name: string; revision: number;
	projectId: string; projectName: string;
	versionId: string; versionName: string;
	fileName: string; fileSize: number;
}
export const createShaderInstallPlan = (name: string, project: string, version: string, revision: number, signal: AbortSignal) =>
	runtimeRequest<ShaderInstallPlan>(`/clients/${encodeURIComponent(name)}/shader-plans`, { method: 'POST', body: JSON.stringify({ project, version, revision }), signal });
export const listModrinthFiles = (name: string, project: string, index: number, signal: AbortSignal, options?: CatalogOptions) =>
	runtimeRequest<CurseForgePage<ModrinthFile>>(`/clients/${encodeURIComponent(name)}/modrinth/projects/${encodeURIComponent(project)}/files?${new URLSearchParams({ index: String(index), ...catalogQuery(options) })}`, { signal });
export const createModrinthInstallPlan = (name: string, project: string, file: string, signal: AbortSignal) =>
	runtimeRequest<ModrinthInstallPlan>(`/clients/${encodeURIComponent(name)}/modrinth/projects/${encodeURIComponent(project)}/files/${encodeURIComponent(file)}/plan`, { method: 'POST', signal });
export interface ResourceInstallOperation {
	id: number; name: string; kind: 'mod' | 'shader'; state: 'running' | 'completed' | 'failed' | 'cancelled'; cancelRequested: boolean;
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

export interface ClientLaunch {
	id: number; name: string; offlineTest: boolean; startedAt: number | null; javaDownloadStarted: boolean;
	phase: 'preparing' | 'authenticating' | 'planning' | 'java' | 'running' | 'stopping' | 'exited' | 'failed' | 'cancelled';
	cancelRequested: boolean; javaMajor: number | null;
	javaStage: 'resolving' | 'downloading' | 'extracting' | 'validating' | 'publishing' | 'completed' | null;
	javaDownloadedBytes: number; javaTotalBytes: number | null;
	exitCode: number | null; errorCode: string | null; error: string | null;
}
export const listClientLaunches = (signal: AbortSignal) => runtimeRequest<ClientLaunch[]>('/client-launches', { signal });
export const startClientLaunch = (name: string, accountId: string | null, installJava: boolean, signal: AbortSignal, offlineTest = false) =>
	runtimeRequest<ClientLaunch>(`/clients/${encodeURIComponent(name)}/launch`, { method: 'POST', body: JSON.stringify({ accountId, installJava, offlineTest }), signal });
export const stopClientLaunch = (id: number, signal: AbortSignal) =>
	runtimeRequest<ClientLaunch>(`/client-launches/${id}`, { method: 'DELETE', signal });

export interface PlayStatsSession {
	instanceId: string; startedAt: number; endedAt: number; durationMs: number;
}
export interface PlayStatsInstance { totalMs: number; launches: number; lastPlayedAt: number }
export interface PlayStatsAccount {
	totalMs: number; launches: number; lastPlayedAt: number | null; lastInstanceId: string | null;
	last7dMs: number; instances: Record<string, PlayStatsInstance>; recentSessions: PlayStatsSession[];
}
export interface PlayStatsSnapshot { schemaVersion: number; accounts: Record<string, PlayStatsAccount> }
export const getPlayStats = (signal: AbortSignal) =>
	runtimeRequest<PlayStatsSnapshot>('/play-stats', { signal, cache: 'no-store' });
export const clearPlayStats = (signal: AbortSignal) =>
	runtimeRequest<PlayStatsSnapshot>('/play-stats', { method: 'DELETE', signal });

export interface ClientGameLog { exists: boolean; text: string; totalBytes: number; truncated: boolean }
export const getClientGameLog = (name: string, signal: AbortSignal) =>
	runtimeRequest<ClientGameLog>(`/clients/${encodeURIComponent(name)}/logs/latest`, { signal });
export interface ClientConsoleLog extends ClientGameLog { launchId: number | null; readFailed: boolean }
export const getClientConsoleLog = (name: string, signal: AbortSignal) =>
	runtimeRequest<ClientConsoleLog>(`/clients/${encodeURIComponent(name)}/logs/console`, { signal, cache: 'no-store' });

export interface ClientCrashReportEntry { name: string; size: number; modifiedAt: number | null }
export interface ClientCrashReports { reports: ClientCrashReportEntry[]; truncated: boolean; maxReportBytes: number }
export interface ClientCrashReport { name: string; text: string; totalBytes: number }
export const listClientCrashReports = (name: string, signal: AbortSignal) =>
	runtimeRequest<ClientCrashReports>(`/clients/${encodeURIComponent(name)}/crash-reports`, { signal, cache: 'no-store' });
export const getClientCrashReport = (name: string, filename: string, signal: AbortSignal) =>
	runtimeRequest<ClientCrashReport>(`/clients/${encodeURIComponent(name)}/crash-reports/${encodeURIComponent(filename)}`, { signal, cache: 'no-store' });

export interface ClientLaunchSettings {
	schemaVersion: 1; revision: number; maxMemoryMib: number; initialMemoryMib: number | null;
	resolution: { width: number; height: number } | null;
}
export const getClientLaunchSettings = (name: string, signal: AbortSignal) =>
	runtimeRequest<ClientLaunchSettings>(`/clients/${encodeURIComponent(name)}/launch-settings`, { signal, cache: 'no-store' });
export const saveClientLaunchSettings = (name: string, settings: ClientLaunchSettings, signal: AbortSignal) =>
	runtimeRequest<ClientLaunchSettings>(`/clients/${encodeURIComponent(name)}/launch-settings`, { method: 'PUT', body: JSON.stringify(settings), signal });
export const getDeviceMemory = (signal: AbortSignal) =>
	runtimeRequest<{ totalMib: number | null }>('/system/memory', { signal });
