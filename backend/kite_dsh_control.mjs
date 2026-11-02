import fs from 'node:fs';
import net from 'node:net';
import {createRequire} from 'node:module';
import {pathToFileURL} from 'node:url';

// A private host endpoint into the native inbox. ACP itself allows one prompt
// RPC at a time; agent.steer admits input at the next native model boundary.
export function startControl(ctx, path) {
  const server = net.createServer(socket => {
    let buffer = '';
    socket.on('data', async data => {
      buffer += data.toString();
      if (Buffer.byteLength(buffer) > 2 * 1024 * 1024) { socket.destroy(); return; }
      if (!buffer.endsWith('\n')) return;
      socket.pause();
      try {
        const value = JSON.parse(buffer);
        if (value.method !== 'steer' || typeof value.sessionId !== 'string'
            || typeof value.text !== 'string' || !value.text.trim()) throw new Error('invalid steering');
        const agent = ctx.agents.get(value.sessionId);
        if (!agent) throw new Error('session unavailable');
        if (agent.status === 'idle') { socket.end(JSON.stringify({accepted:false})+'\n'); return; }
        const require = createRequire(process.argv[1]);
        const {createUserMessage} = await import(pathToFileURL(require.resolve('@deepseek-ai/dsh-llm')).href);
        // Recheck after the async import before committing to a live turn.
        if (agent.status === 'idle') { socket.end(JSON.stringify({accepted:false})+'\n'); return; }
        const message = createUserMessage({content:[{type:'text',text:value.text}],source:{kind:'user'}});
        agent.steer(message);
        await ctx.sessions.flush(agent.session);
        socket.end(JSON.stringify({accepted:true,messageId:message.id})+'\n');
      } catch { socket.end(JSON.stringify({error:'steering_unavailable'})+'\n'); }
    });
    socket.on('error', () => {});
  });
  server.on('error', () => {});
  server.listen(path, () => fs.chmodSync(path, 0o600));
  return () => server.close();
}
