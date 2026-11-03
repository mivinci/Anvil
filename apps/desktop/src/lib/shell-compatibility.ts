import { defaultLocale, messages, type Locale } from './i18n/index.ts';
import type { AppInfo, AppUpdateStatus } from './shell';

export type ShellCapability = 'window.theme' | 'window.effects' | 'updates.app' | 'updates.shell' | 'logs.open' | 'external-url.open';
const legacyCapabilities: readonly ShellCapability[] = ['window.theme', 'updates.app', 'updates.shell', 'logs.open', 'external-url.open'];

/** Explicit capabilities win; only the known 1.0 contract receives legacy defaults. */
export function supportsShellCapability(info: Pick<AppInfo, 'shellApiVersion' | 'capabilities'> | null, capability: ShellCapability): boolean {
	if (!info) return false;
	if (info.capabilities !== undefined) return Array.isArray(info.capabilities) && info.capabilities.includes(capability);
	return info.shellApiVersion === '1.0.0' && legacyCapabilities.includes(capability);
}

export function shellUpgradeRequired(status: Pick<AppUpdateStatus, 'failure'> | null | undefined): boolean {
	return status?.failure === 'shell_upgrade_required';
}

export function shellUpgradeMessage(status: Pick<AppUpdateStatus, 'availableVersion'>, locale: Locale = defaultLocale): string {
	return messages[locale].settings.shellUpgrade(status.availableVersion);
}
