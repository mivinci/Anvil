import { runtimeErrorCode } from './runtime-errors.ts';
import type { CurseForgeFile, CurseForgeInstallPlan, ModProjectSummary, ModSource } from './runtime';

export type ModInstallFile = Pick<CurseForgeFile, 'displayName' | 'fileName' | 'fileLength' | 'fileDate' | 'releaseType'> & { id: string | number };
export type ModInstallPlan = Omit<CurseForgeInstallPlan, 'items' | 'files' | 'optionalProjects'> & {
	items: { project: ModProjectSummary; file: ModInstallFile; direct: boolean }[];
	files: { projectId: string | number; fileId: string | number; projectName: string; filename: string; size: number; direct: boolean; reuse: boolean }[];
	optionalProjects: (string | number)[];
};
export interface ModInstallContext { source: ModSource; name: string; gameVersion: string; project: string; replaceFrom?: string | number | undefined }
export interface ModPlanSnapshot {
	selected: ModInstallFile | null; plan: ModInstallPlan | null;
	busy: boolean; error: string | null; errorCode?: string | null; expired: boolean; accepted: boolean;
	optionalChoices?: (string | number)[]; selectedOptional?: (string | number)[];
}
interface Dependencies {
	read: (context: ModInstallContext, file: ModInstallFile, signal: AbortSignal, optional: readonly (string | number)[]) => Promise<ModInstallPlan>;
	start: (name: string, id: number) => Promise<boolean>;
	canSubmit: (id: number) => boolean;
	now?: () => number;
}

/** Page-owned plan reads; accepted installation belongs to the window's task state. */
export class ModInstallPlanState {
	private context: ModInstallContext | null = null;
	private controller: AbortController | null = null;
	private timer: ReturnType<typeof setTimeout> | null = null;
	private generation = 0;
	private submitting = false;
	private deadline = 0;
	private snapshot: ModPlanSnapshot = { selected: null, plan: null, busy: false, error: null, errorCode: null, expired: false, accepted: false };
	private publish: (value: ModPlanSnapshot) => void;
	private deps: Dependencies;
	constructor(deps: Dependencies, publish: (value: ModPlanSnapshot) => void) { this.deps = deps; this.publish = publish; }
	private now() { return this.deps.now?.() ?? performance.now(); }
	private emit(change: Partial<ModPlanSnapshot>) { this.snapshot = { ...this.snapshot, ...change }; this.publish(this.snapshot); }
	private cancelRead() {
		this.generation++; this.controller?.abort(); this.controller = null;
		if (this.timer) clearTimeout(this.timer); this.timer = null;
	}
	bind(context: ModInstallContext | null) {
		this.cancelRead(); this.context = context ? { ...context } : null; this.deadline = 0;
		this.emit({ selected: null, plan: null, busy: false, error: null, errorCode: null, expired: false, accepted: false, optionalChoices: [], selectedOptional: [] });
	}
	dispose() { this.cancelRead(); this.context = null; }
	clear() { const context = this.context; this.bind(context); }
	async select(file: ModInstallFile, optional?: (string | number)[]) {
		const context = this.context;
		if (!context || this.submitting) return;
		const sameFile = this.snapshot.selected?.id === file.id;
		const chosen = optional ?? (sameFile ? this.snapshot.selectedOptional : undefined) ?? [];
		const optionalChoices = sameFile ? this.snapshot.optionalChoices ?? [] : [];
		this.cancelRead();
		const generation = this.generation, started = this.now();
		const controller = new AbortController(); this.controller = controller;
		this.emit({ selected: file, plan: null, busy: true, error: null, errorCode: null, expired: false, accepted: false, optionalChoices, selectedOptional: chosen });
		try {
			const plan = await this.deps.read(context, file, AbortSignal.any([controller.signal, AbortSignal.timeout(55_000)]), chosen);
			if (controller.signal.aborted || generation !== this.generation) return;
			const root = plan.items.find(item => item.direct && String(item.project.id) === context.project && item.file.id === file.id);
			if (!root || plan.gameVersion !== context.gameVersion || !Number.isSafeInteger(plan.planId) || plan.planId <= 0 || !Number.isFinite(plan.expiresInMs) || plan.expiresInMs <= 0 || plan.expiresInMs > 600_000) throw new Error('Installation plan does not match the current instance or version.');
			const replacement = plan.replacement;
			if (context.replaceFrom !== undefined) {
				if (!replacement || String(replacement.projectId) !== context.project || replacement.fromFileId !== context.replaceFrom || replacement.toFileId !== file.id || replacement.toFilename !== root.file.fileName || !replacement.fromFilename || typeof replacement.enabled !== 'boolean' || !Array.isArray(replacement.dependents) || !replacement.dependents.every(name => typeof name === 'string')) {
					this.emit({ error: 'Replacement preview did not match the selected versions.', errorCode: 'replacement_preview_mismatch' });
					return;
				}
			} else if (replacement) {
				this.emit({ error: 'Unexpected replacement preview.', errorCode: 'replacement_preview_mismatch' });
				return;
			}
			const echoed = plan.selectedOptionalProjects ?? [];
			if (echoed.length !== chosen.length || chosen.some(id => !echoed.includes(id) || !plan.items.some(item => item.project.id === id))) {
				throw new Error('Installation plan does not match the selected optional dependencies.');
			}
			this.deadline = started + plan.expiresInMs;
			const remaining = this.deadline - this.now();
			this.emit({ plan, expired: remaining <= 0, optionalChoices: plan.selectedOptionalProjects === undefined ? [] : plan.optionalProjects });
			if (remaining > 0) this.timer = setTimeout(() => { if (generation !== this.generation) return; this.timer = null; this.emit({ expired: true }); }, remaining);
		} catch (error) {
			if (!controller.signal.aborted && generation === this.generation) this.emit({ error: error instanceof Error ? error.message : 'Could not prepare installation plan.', errorCode: runtimeErrorCode(error) });
		} finally {
			if (generation === this.generation) { this.controller = null; this.emit({ busy: false }); }
		}
	}
	async setOptional(id: string | number, included: boolean) {
		const { selected, optionalChoices, selectedOptional = [], busy, accepted } = this.snapshot;
		if (!selected || busy || accepted || !optionalChoices?.includes(id)) return;
		const next = included ? [...new Set([...selectedOptional, id])] : selectedOptional.filter(value => value !== id);
		if (next.length <= 8) await this.select(selected, next);
	}
	async confirm() {
		const { plan } = this.snapshot, context = this.context;
		if (!plan || !context || this.submitting || this.snapshot.busy || this.snapshot.accepted) return false;
		if (this.snapshot.expired || this.now() >= this.deadline) { this.emit({ expired: true }); return false; }
		if (!this.deps.canSubmit(plan.planId)) return false;
		const generation = this.generation;
		this.submitting = true;
		try {
			const accepted = await this.deps.start(context.name, plan.planId);
			if (accepted && generation === this.generation) {
				if (this.timer) clearTimeout(this.timer); this.timer = null;
				this.emit({ accepted: true });
			}
			return accepted;
		} finally { this.submitting = false; }
	}
}
