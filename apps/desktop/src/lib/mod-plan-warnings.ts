const routine = new Set([
	'仅安装兼容的 Fabric 主 JAR 和必需依赖；不自动升级或替换已有项目。',
	'平台标签不包含完整的模组版本约束与客户端侧要求；尚未与已导入的本地模组求解兼容性。',
	'可选依赖不会自动加入；工具和内嵌依赖不单独下载。',
]);

/** Only known informational messages can be folded away; new warnings stay visible. */
export function modPlanWarnings(warnings: string[]) {
	return {
		details: warnings.filter(value => routine.has(value)),
		important: warnings.filter(value => !routine.has(value) && !value.startsWith('这是只读依赖预览')),
	};
}
