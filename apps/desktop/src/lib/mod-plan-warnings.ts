import { defaultLocale, messages, type Locale } from './i18n/index.ts';

type WarningDetail = { code: string; message: string };

/** Codes are authoritative. Unknown/legacy notices stay visible with their raw detail. */
export function modPlanWarnings(warnings: string[], warningDetails?: WarningDetail[], locale: Locale = defaultLocale) {
	const copy = messages[locale].mods.discovery;
	const details: string[] = [], technical: string[] = [];
	const known = {
		fabric_required_only: copy.fabricRequiredOnly,
		platform_constraints_incomplete: copy.platformConstraintsIncomplete,
		optional_dependencies_excluded: copy.optionalDependenciesExcluded,
	};
	if (warningDetails?.length) {
		for (const warning of warningDetails) {
			if (warning.code === 'read_only_preview') continue;
			if (Object.hasOwn(known, warning.code)) details.push(known[warning.code as keyof typeof known]);
			else technical.push(warning.message || warning.code);
		}
	} else technical.push(...warnings);
	return { details, important: technical.length ? [copy.unknownWarning] : [], technical };
}
