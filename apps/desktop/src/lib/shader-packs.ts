import { messages, defaultLocale, type Locale } from './i18n/index.ts';
import type { ClientShaderPackChange, ClientShaderPacks, ShaderEnvironment, ShaderMod } from './runtime';
import { validateLocalPacks } from './local-pack.ts';

export const shaderEnvironmentLabels = (locale: Locale = defaultLocale): Record<ShaderEnvironment['status'], string> => ({
	requires_fabric: messages[locale].instance.detail.requiresFabric, missing_iris: messages[locale].instance.detail.missingIris, iris_disabled: messages[locale].instance.detail.irisDisabled,
	missing_dependencies: messages[locale].instance.detail.missingDependencies, unconfirmed: messages[locale].instance.detail.environmentUnconfirmed, ready: messages[locale].instance.detail.shadersReady,
});
export function validateShaderPackChange(value: ClientShaderPackChange): ClientShaderPackChange {
	return validateLocalPacks(value);
}
function validMod(mod: ShaderMod | null) {
	return mod === null || (mod && typeof mod.version === 'string' && mod.version.length > 0 && mod.version.length <= 128
		&& !/[\u0000-\u001f\u007f]/.test(mod.version) && typeof mod.enabled === 'boolean');
}
export function validateShaderPacks(value: ClientShaderPacks): ClientShaderPacks {
	validateShaderPackChange(value);
	const env = value.environment;
	if (!env || !Object.hasOwn(shaderEnvironmentLabels(), env.status) || env.managedOnly !== true
		|| (env.reasonCode !== undefined && (typeof env.reasonCode !== 'string' || !/^[a-z][a-z0-9_]{0,63}$/.test(env.reasonCode)))
		|| (env.reasonParams !== undefined && (!env.reasonParams || typeof env.reasonParams !== 'object' || Array.isArray(env.reasonParams) || Object.keys(env.reasonParams).length > 8 || Object.entries(env.reasonParams).some(([key, value]) => key.length > 32 || typeof value !== 'string' || value.length > 256)))
		|| typeof env.reason !== 'string' || env.reason.length > 4096 || !validMod(env.iris) || !validMod(env.sodium)
		|| (env.status === 'ready' && !env.iris?.enabled)
		|| (env.status === 'iris_disabled' && (!env.iris || env.iris.enabled))
		|| (env.status === 'missing_iris' && env.iris !== null)) throw new Error(messages[defaultLocale].instance.detail.invalidEnvironment);
	return value;
}

/** A failed post-commit refresh must not turn a confirmed write into an unknown write. */
export async function refreshAfterShaderChange(
	change: ClientShaderPackChange,
	read: (signal: AbortSignal) => Promise<ClientShaderPacks>,
	signal: AbortSignal,
): Promise<{ snapshot: ClientShaderPacks | null; error: string | null }> {
	try {
		const snapshot = validateShaderPacks(await read(signal));
		signal.throwIfAborted();
		if (snapshot.revision < change.revision) throw new Error(messages[defaultLocale].instance.detail.staleShaders);
		return { snapshot, error: null };
	} catch (reason) {
		signal.throwIfAborted();
		return { snapshot: null, error: reason instanceof Error ? reason.message : messages[defaultLocale].instance.detail.refreshFailed };
	}
}

/** Older Runtime statuses remain usable; unknown reason codes get a safe explanation. */
export function shaderEnvironmentReason(env: ShaderEnvironment, locale: Locale = defaultLocale): string {
	if (!env.reasonCode) return messages[locale].instance.environmentHelp[env.status];
	const reasons = messages[locale].errors.shaderReasons;
	return Object.hasOwn(reasons, env.reasonCode)
		? reasons[env.reasonCode as keyof typeof reasons](env.reasonParams ?? {})
		: reasons.shader_unconfirmed({});
}
