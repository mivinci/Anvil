import type { ClientShaderPackChange, ClientShaderPacks, ShaderEnvironment, ShaderMod } from './runtime';
import { validateLocalPacks } from './local-pack.ts';

export const shaderEnvironmentLabels: Record<ShaderEnvironment['status'], string> = {
	requires_fabric: '此实例需要光影支持', missing_iris: '尚未安装 Iris', iris_disabled: 'Iris 尚未启用',
	missing_dependencies: '光影依赖尚未就绪', unconfirmed: '光影环境待确认', ready: '光影支持已就绪',
};
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
	if (!env || !Object.hasOwn(shaderEnvironmentLabels, env.status) || env.managedOnly !== true
		|| typeof env.reason !== 'string' || env.reason.length > 4096 || !validMod(env.iris) || !validMod(env.sodium)
		|| (env.status === 'ready' && !env.iris?.enabled)
		|| (env.status === 'iris_disabled' && (!env.iris || env.iris.enabled))
		|| (env.status === 'missing_iris' && env.iris !== null)) throw new Error('光影环境信息格式不正确。');
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
		if (snapshot.revision < change.revision) throw new Error('读取到旧的光影清单，请刷新。');
		return { snapshot, error: null };
	} catch (reason) {
		signal.throwIfAborted();
		return { snapshot: null, error: reason instanceof Error ? reason.message : '刷新失败。' };
	}
}
