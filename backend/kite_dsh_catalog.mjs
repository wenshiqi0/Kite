import fs from 'node:fs';
import {startControl} from './kite_dsh_control.mjs';

// Read the native service once per process. Never serialize call config or keys.
export const inject = ['llm', 'agents', 'sessions'];
export function apply(ctx, config) {
  const llm = ctx.llm;
  ctx.effect(() => {
    let stopped = false;
    setImmediate(async () => {
      try {
        await ctx.get('loader')?.await();
        if (config.control) ctx.effect(() => startControl(ctx, config.control));
        const models = [];
        for (const provider of llm.listProviders()) {
          if (provider.id !== config.provider) continue;
          for (const entry of await llm.listModels(provider.id)) {
            const info = await llm.resolveModelInfo(provider.id, entry.id);
            models.push({
              model: JSON.stringify([provider.id, entry.id]),
              displayName: info.name ?? entry.id,
              description: info.description ?? '', hidden: false, isDefault: false,
              inputModalities: info.inputModalities ?? ['text'],
              defaultReasoningEffort: info.reasoning?.defaultEffort ?? '',
              supportedReasoningEfforts: [
                ...(info.reasoning && !info.reasoning.defaultEffort ? [{reasoningEffort: '', description: 'Provider default'}] : []),
                ...(info.reasoning?.efforts ?? []).map(e => ({
                reasoningEffort: e.id, description: e.description ?? e.name,
              })),
              ],
            });
          }
        }
        if (!stopped) fs.writeFileSync(config.output, JSON.stringify({models}), {mode: 0o600});
      } catch {
        if (!stopped) fs.writeFileSync(config.output, JSON.stringify({error: 'catalog_unavailable'}), {mode: 0o600});
      }
    });
    return () => { stopped = true; };
  });
}
