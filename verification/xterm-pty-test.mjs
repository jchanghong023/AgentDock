import {connect,delay} from './xterm-cdp.mjs';
import {writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
const c=await connect(Number(process.argv[2]||19342));
const contains=text=>`(()=>{const b=[...agentdock.terminals.values()][0].term.buffer.active;return Array.from({length:b.length},(_,i)=>b.getLine(i).translateToString(true)).join(' ').includes(${JSON.stringify(text)})})()`;
async function input(text){await c.send('Input.insertText',{text});await c.key('Enter','Enter',13);}
try {
  await c.waitFor(contains('FIXTURE_READY'));
  await input('中文往返');await c.waitFor(contains('ECHO:中文往返'));
  await input('alternate');await c.waitFor(contains('ALTERNATE_中文'));
  assert.equal(await c.evaluate('[...agentdock.terminals.values()][0].term.buffer.active.type'),'alternate');
  await input('normal');await c.waitFor(contains('NORMAL_RESTORED'));
  assert.equal(await c.evaluate('[...agentdock.terminals.values()][0].term.buffer.active.type'),'normal');
  // >32 KiB, crosses UTF-8 and input-batch boundaries; compare bytes at child process.
  const payload='中文🙂abc'.repeat(10000);const digest=createHash('sha256').update(payload).digest('hex');
  await input('hash:'+payload);await c.waitFor(contains('HASH:'+digest));
  await c.evaluate(`window.frameProbe={last:performance.now(),max:0};window.frameProbeId=setInterval(()=>{const p=frameProbe,now=performance.now();p.max=Math.max(p.max,now-p.last);p.last=now;},16)`);
  const start=performance.now();await input('bulk');await c.waitFor(contains('BULK_DONE'),30000);const bulkMs=performance.now()-start;
  const text=await c.text();const rows=[...text.matchAll(/ROW:(\d{5}):中文你好:abcdefghijklmnopqrstuvwxyz0123456789/g)].map(m=>Number(m[1]));
  assert.equal(rows.length,10000);assert(rows.every((n,i)=>n===i));
  const maxTimerGap=await c.evaluate('clearInterval(frameProbeId);frameProbe.max');
  await c.evaluate('[...agentdock.terminals.values()][0].term.scrollToTop()');await delay(80);await c.screenshot('.tmp/xterm-scrollback-top.png');
  assert.equal(await c.evaluate('[...agentdock.terminals.values()][0].term.buffer.active.viewportY'),0);
  await c.evaluate('[...agentdock.terminals.values()][0].term.scrollToBottom()');
  await input('after-bulk');await c.waitFor(contains('ECHO:after-bulk'));
  const result={chineseRoundtrip:true,alternateBuffer:true,largeInputBytes:Buffer.byteLength(payload),sha256Match:true,bulkLines:rows.length,bulkMs,maxTimerGap,scrollback:true,inputAfterBulk:true};
  await writeFile('.tmp/xterm-pty-results.json',JSON.stringify(result,null,2));console.log(JSON.stringify(result,null,2));
}finally{c.close();}
