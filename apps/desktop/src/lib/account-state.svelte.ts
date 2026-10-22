import { createContext } from 'svelte';
import {
	cancelAccountLogin, getAccountLoginConfiguration, getAccountLoginOperation, getActiveAccountLogin,
	listAccounts, logoutAccount, selectAccount, startAccountLogin, RuntimeRequestError,
	type AccountLoginConfiguration, type AccountLoginOperation, type AccountSummary,
} from './runtime';

const POLL_DELAY_MS = 1_200;
const REQUEST_TIMEOUT_MS = 15_000;

type AccountAction = { kind: 'load' } | { kind: 'switch' | 'logout'; id: string };
export interface AccountIssue {
	message: string;
	detail: string;
	retry: AccountAction;
}

function detail(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

function loginFailure(message: string | null): string {
	if (message?.includes('invalid app registration')) return '当前应用尚未获准访问 Minecraft 服务，需要完成应用注册审核。';
	if (message?.includes('Minecraft Services login was rejected')) return 'Minecraft 服务拒绝了登录请求，请展开错误详情查看状态码。';
	if (message?.includes('no Minecraft Java profile')) return '此账户尚无 Minecraft Java 版游戏资料。';
	if (message?.includes('declined')) return '你已拒绝本次授权，可以重新登录。';
	if (message?.includes('expired')) return '验证码已过期，请重新登录。';
	if (message?.includes('credential vault')) return '无法访问系统凭证库，请检查系统权限后重试。';
	return '登录未完成，请稍后重试。';
}

/** Window-owned public identities and transient login state. No credentials are stored here. */
export class AccountState {
	accounts = $state<AccountSummary[]>([]);
	loaded = $state(false);
	loading = $state(false);
	pendingAction = $state<'switch' | 'logout' | null>(null);
	error = $state<AccountIssue | null>(null);
	configuration = $state<AccountLoginConfiguration | null>(null);
	configurationLoading = $state(false);
	login = $state<AccountLoginOperation | null>(null);
	loginStarting = $state(false);
	loginCancelling = $state(false);
	loginPolling = $state(false);
	loginError = $state<string | null>(null);
	loginErrorDetail = $state<string | null>(null);
	loginDialogOpen = $state(false);

	private disposed = false;
	private cancelAfterStart = false;
	private flowVersion = 0;
	private pollTimer: ReturnType<typeof setTimeout> | null = null;
	private pollRequest: AbortController | null = null;
	private requests = new Set<AbortController>();

	get currentAccount(): AccountSummary | null {
		return this.accounts.find((account) => account.selected) ?? null;
	}

	get loginRunning(): boolean {
		return this.login?.state === 'running';
	}

	get loginCompleted(): boolean {
		return this.login?.state === 'completed';
	}

	get busy(): boolean {
		return this.loading || this.pendingAction !== null || this.loginStarting || this.loginRunning;
	}

	get canCancelLogin(): boolean {
		return !this.loginCancelling && !this.login?.cancelRequested
			&& (this.loginStarting || (this.loginRunning && this.login?.stage !== 'saving_account'));
	}

	/** Read requests may be aborted. The start POST is deliberately excluded: its ID is needed for cleanup. */
	private async request<T>(run: (signal: AbortSignal) => Promise<T>): Promise<T> {
		const controller = new AbortController();
		if (this.disposed) controller.abort();
		this.requests.add(controller);
		try {
			return await run(AbortSignal.any([controller.signal, AbortSignal.timeout(REQUEST_TIMEOUT_MS)]));
		} finally {
			this.requests.delete(controller);
		}
	}

	async refreshAccounts(): Promise<boolean> {
		if (this.disposed || this.busy) return false;
		this.loading = true;
		this.error = null;
		try {
			const accounts = await this.request((signal) => listAccounts({ signal }));
			if (this.disposed) return false;
			this.accounts = accounts;
			this.loaded = true;
			return true;
		} catch (error) {
			if (!this.disposed) {
				this.error = { message: '暂时无法读取账户，请稍后重试。', detail: detail(error), retry: { kind: 'load' } };
			}
			return false;
		} finally {
			if (!this.disposed) this.loading = false;
		}
	}

	async switchAccount(id: string): Promise<boolean> {
		return this.mutateAccount('switch', id);
	}

	async logout(id: string): Promise<boolean> {
		return this.mutateAccount('logout', id);
	}

	private async mutateAccount(kind: 'switch' | 'logout', id: string): Promise<boolean> {
		if (this.disposed || this.busy) return false;
		this.pendingAction = kind;
		this.error = null;
		try {
			const accounts = await this.request((signal) => kind === 'switch'
				? selectAccount(id, { signal }) : logoutAccount(id, { signal }));
			if (this.disposed) return false;
			this.accounts = accounts;
			this.loaded = true;
			return true;
		} catch (error) {
			if (!this.disposed) {
				this.error = {
					message: kind === 'switch' ? '切换账户未完成，请重试。' : '退出登录未确认，请重新读取账户状态。',
					detail: detail(error), retry: { kind, id },
				};
			}
			return false;
		} finally {
			if (!this.disposed) this.pendingAction = null;
		}
	}

	async retryAccountAction(): Promise<boolean> {
		if (this.disposed || this.busy) return false;
		const action = this.error?.retry ?? { kind: 'load' };
		// A failed response does not prove the mutation failed. Reconcile before repeating it.
		if (!await this.refreshAccounts()) {
			if (this.error) this.error = { ...this.error, retry: action };
			return false;
		}
		if (action.kind === 'switch' && this.currentAccount?.id !== action.id) {
			return this.switchAccount(action.id);
		}
		if (action.kind === 'logout' && this.accounts.some((account) => account.id === action.id)) {
			return this.logout(action.id);
		}
		return true;
	}

	async loadLoginConfiguration(): Promise<boolean> {
		if (this.disposed || this.configurationLoading) return false;
		this.configurationLoading = true;
		this.loginError = null;
		this.loginErrorDetail = null;
		try {
			const configuration = await this.request((signal) => getAccountLoginConfiguration({ signal }));
			if (this.disposed) return false;
			this.configuration = configuration;
			if (!configuration.enabled) this.loginError = '当前应用的 Microsoft 登录配置不可用。';
			return configuration.enabled;
		} catch (error) {
			if (!this.disposed) {
				this.configuration = null;
				this.loginError = '暂时无法获取登录配置，请重试。';
				this.loginErrorDetail = detail(error);
			}
			return false;
		} finally {
			if (!this.disposed) this.configurationLoading = false;
		}
	}

	async startLogin(): Promise<boolean> {
		if (this.disposed || this.busy || this.configurationLoading) return false;
		this.loginStarting = true;
		this.cancelAfterStart = false;
		this.loginError = null;
		this.loginErrorDetail = null;
		this.login = null;
		this.stopPolling();
		try {
			if (!await this.loadLoginConfiguration() || this.disposed || this.cancelAfterStart) return false;
			const operation = await startAccountLogin();
			if (this.disposed) {
				await this.cancelAbandonedLogin(operation);
				return false;
			}
			this.applyLogin(operation);
			if (this.cancelAfterStart) return await this.cancelLogin();
			this.schedulePoll();
			return true;
		} catch (error) {
			// The POST can have succeeded even if its response was lost. Recover the
			// single active operation instead of creating a duplicate or losing its ID.
			if (!(error instanceof RuntimeRequestError) || error.status === 409 || error.status >= 500) {
				try {
					const { operation } = await getActiveAccountLogin({ signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
					if (operation) {
						if (this.disposed) {
							await this.cancelAbandonedLogin(operation);
							return false;
						}
						this.applyLogin(operation);
						if (this.cancelAfterStart) return await this.cancelLogin();
						this.schedulePoll();
						return true;
					}
				} catch {
					// Preserve the original start failure; a later retry can recover again.
				}
			}
			if (!this.disposed) {
				this.loginError = '暂时无法开始 Microsoft 登录，请重试。';
				this.loginErrorDetail = detail(error);
			}
			return false;
		} finally {
			if (!this.disposed) {
				this.loginStarting = false;
				if (this.loginCompleted) void this.refreshAccounts();
			}
		}
	}

	resetLogin(): boolean {
		if (this.disposed || this.loginRunning || this.loginStarting || this.loginCancelling) return false;
		this.stopPolling();
		this.login = null;
		this.loginError = null;
		this.loginErrorDetail = null;
		this.cancelAfterStart = false;
		return true;
	}

	openLoginDialog() {
		if (this.disposed) return;
		if (!this.loginRunning && !this.loginStarting) {
			this.resetLogin();
			void this.loadLoginConfiguration();
		}
		this.loginDialogOpen = true;
	}

	closeLoginDialog(): boolean {
		if (this.loginRunning && this.login?.stage === 'saving_account') return false;
		this.loginDialogOpen = false;
		if (this.canCancelLogin) void this.cancelLogin();
		return true;
	}

	async cancelLogin(): Promise<boolean> {
		if (this.disposed || !this.canCancelLogin) return false;
		this.cancelAfterStart = true;
		this.stopPolling();
		this.loginError = null;
		this.loginErrorDetail = null;
		const operation = this.login;
		// An in-flight start must first return its operation ID; startLogin then cancels it.
		if (!operation || operation.state !== 'running') return true;
		this.loginCancelling = true;
		this.login = { ...operation, authorization: null };
		try {
			const cancelled = await this.request((signal) => cancelAccountLogin(operation.id, { signal }));
			if (this.disposed) return false;
			this.applyLogin(cancelled);
			// A cancellation acknowledgement can still be running while Core cleans up.
			this.schedulePoll();
			return true;
		} catch (error) {
			if (!this.disposed) {
				this.loginError = '取消尚未确认，请重试取消或重新查询进度。';
				this.loginErrorDetail = detail(error);
			}
			return false;
		} finally {
			if (!this.disposed) this.loginCancelling = false;
		}
	}

	async resumeLogin(): Promise<boolean> {
		if (this.disposed || !this.loginRunning || this.loginCancelling || this.loginStarting) return false;
		this.stopPolling();
		return this.pollLogin(this.flowVersion);
	}

	private applyLogin(operation: AccountLoginOperation) {
		this.login = operation;
		this.loginError = null;
		this.loginErrorDetail = null;
		if (operation.state === 'failed') {
			this.loginError = loginFailure(operation.error);
			this.loginErrorDetail = operation.error;
		} else if (operation.state === 'cancelled' && operation.error?.includes('cleanup')) {
			this.loginError = '登录已取消，但凭据清理未完成，请检查系统凭证库。';
			this.loginErrorDetail = operation.error;
		}
		if (operation.state !== 'running') this.stopPolling();
		if (operation.state === 'completed' && operation.account) {
			const account = operation.account;
			this.accounts = [
				...this.accounts.filter((item) => item.id !== account.id).map((item) => ({ ...item, selected: false })),
				{ ...account, selected: true },
			];
			// The login result is authoritative even if reloading the remaining identities fails.
			void this.refreshAccounts();
		}
	}

	private schedulePoll() {
		if (this.disposed || !this.loginRunning || this.pollTimer || this.pollRequest) return;
		const version = this.flowVersion;
		this.pollTimer = setTimeout(() => {
			this.pollTimer = null;
			void this.pollLogin(version);
		}, POLL_DELAY_MS);
	}

	private async pollLogin(version: number): Promise<boolean> {
		const operation = this.login;
		if (this.disposed || !operation || operation.state !== 'running' || version !== this.flowVersion || this.pollRequest) return false;
		const controller = new AbortController();
		this.pollRequest = controller;
		this.loginPolling = true;
		let success = false;
		try {
			const snapshot = await getAccountLoginOperation(operation.id, {
				signal: AbortSignal.any([controller.signal, AbortSignal.timeout(REQUEST_TIMEOUT_MS)]),
			});
			if (this.disposed || version !== this.flowVersion) return false;
			this.applyLogin(snapshot);
			success = true;
			return true;
		} catch (error) {
			if (!this.disposed && version === this.flowVersion) {
				if (error instanceof RuntimeRequestError && error.status === 404) {
					this.login = { ...operation, state: 'failed', authorization: null, error: detail(error) };
					this.stopPolling();
					this.loginError = '登录任务已失效，请重新登录。';
				} else {
					this.loginError = '暂时无法获取登录进度，请重新查询；授权尚未确认结束。';
				}
				this.loginErrorDetail = detail(error);
			}
			return false;
		} finally {
			if (this.pollRequest === controller) {
				this.pollRequest = null;
				this.loginPolling = false;
				if (success) this.schedulePoll();
			}
		}
	}

	private stopPolling() {
		this.flowVersion += 1;
		if (this.pollTimer) clearTimeout(this.pollTimer);
		this.pollTimer = null;
		this.pollRequest?.abort();
		this.pollRequest = null;
		this.loginPolling = false;
	}

	private async cancelAbandonedLogin(operation: AccountLoginOperation) {
		if (operation.state !== 'running' || operation.stage === 'saving_account') return;
		try {
			await cancelAccountLogin(operation.id, { signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
		} catch (error) {
			console.error('Could not confirm cancellation of the abandoned sign-in:', detail(error));
		}
	}

	dispose() {
		if (this.disposed) return;
		this.disposed = true;
		this.cancelAfterStart = true;
		this.stopPolling();
		for (const controller of this.requests) controller.abort();
		this.requests.clear();
		if (this.login) {
			void this.cancelAbandonedLogin(this.login);
			this.login = { ...this.login, authorization: null };
		}
	}
}

export const [getAccountState, setAccountState] = createContext<AccountState>();
