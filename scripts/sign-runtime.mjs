import { signRuntime } from './lib/runtime-signing.mjs';

const [binary, target] = process.argv.slice(2);
if (!binary || !target) throw new Error('Usage: node scripts/sign-runtime.mjs <binary> <target-triple>');
signRuntime(binary, target);
