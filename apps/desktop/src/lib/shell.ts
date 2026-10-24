import { invoke } from '@tauri-apps/api/core';

export interface AppInfo {
	appId: string;
	appVersion: string | null;
	bundledAppVersion: string;
	installed: boolean;
	targetPlatform: string;
	shellVersion: string;
	shellApiVersion: string;
}

export type AppUpdateState =
	| 'not_configured'
	| 'idle'
	| 'checking'
	| 'downloading'
	| 'verifying'
	| 'staged'
	| 'up_to_date'
	| 'failed'
	| 'cancelled';

export type AppUpdateFailure =
	| 'storage_unavailable'
	| 'network'
	| 'invalid_signature'
	| 'invalid_release'
	| 'download'
	| 'invalid_package'
	| 'version_conflict'
	| 'cancelled'
	| 'activation_failed';

export interface AppUpdateStatus {
	state: AppUpdateState;
	failure: AppUpdateFailure | null;
	currentVersion: string | null;
	availableVersion: string | null;
	downloadedBytes: number;
	totalBytes: number | null;
}

export interface UpdateStatus {
	shell: ShellUpdateStatus;
	app: AppUpdateStatus;
}

export type ShellUpdateState = 'not_configured' | 'idle' | 'checking' | 'available' | 'up_to_date' | 'failed';

export interface ShellUpdateStatus {
	state: ShellUpdateState;
	failure: 'storage_unavailable' | 'network' | 'invalid_signature' | 'invalid_release' | 'cancelled' | null;
	currentVersion: string;
	availableVersion: string | null;
	releaseUrl: string | null;
}

export interface UpdateCheckResult {
	outcome: 'started' | 'already_running' | 'recently_checked' | 'not_configured' | 'failed';
	shellOutcome: 'started' | 'already_running' | 'recently_checked' | 'not_configured' | 'failed';
}

export interface RuntimeStatus {
	state: 'not_installed' | 'starting' | 'ready' | 'failed' | 'stopped';
	failure:
		| 'storage_unavailable'
		| 'bundled_app_install_failed'
		| 'invalid_manifest'
		| 'sidecar_missing'
		| 'sidecar_not_executable'
		| 'secure_random_unavailable'
		| 'spawn_failed'
		| 'startup_timeout'
		| 'invalid_ready_message'
		| 'incompatible_api'
		| 'process_exited'
		| 'process_monitor_failed'
		| null;
	apiVersion: string | null;
	runtimeVersion: string | null;
	availableRuntimeVersion: string | null;
}

export interface RuntimeConnection {
	baseUrl: string;
	token: string;
	apiVersion: string;
}

export interface LoggingStatus {
	directory: string | null;
	fileLoggingAvailable: boolean;
}

/** Typed access to the generic native Shell API. */
export const shell = {
	getLoggingStatus: () => invoke<LoggingStatus>('shell_get_logging_status'),
	openLogDirectory: () => invoke<void>('shell_open_log_directory'),
	getAppInfo: () => invoke<AppInfo>('shell_get_app_info'),
	openExternalUrl: (url: string) => invoke<void>('shell_open_external_url', { url }),
	getUpdateStatus: () => invoke<UpdateStatus>('shell_get_update_status'),
	checkForUpdates: () => invoke<UpdateCheckResult>('shell_check_for_updates'),
	cancelUpdate: () => invoke<boolean>('shell_cancel_update'),
	restartForUpdate: () => invoke<void>('shell_restart_for_update'),
	getRuntimeStatus: () => invoke<RuntimeStatus>('shell_get_runtime_status'),
	getRuntimeConnection: () => invoke<RuntimeConnection>('shell_get_runtime_connection'),
	confirmAppReady: () => invoke<boolean>('shell_confirm_app_ready'),
	reportAppStartupFailure: () => invoke<boolean>('shell_report_app_startup_failure')
};
