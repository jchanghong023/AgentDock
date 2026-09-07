import {connect, delay} from './xterm-cdp.mjs';
import {writeFile} from 'node:fs/promises';
const c = await connect(Number(process.argv[2] || 19341));
try {
  await c.waitFor('agentdock.terminals.size === 1');
  if ((await c.text()).includes('All available models')) { await c.key('Escape','Escape',27); await delay(300); }
  await c.evaluate(`(() => {
    const pane = [...agentdock.terminals.values()][0]; window.modelSamples = [];
    window.modelMeasure = null;
    document.addEventListener('keydown', event => {
      if (event.key === 'Enter') window.modelMeasure = {start:performance.now(), maxFrameGap:0, lastFrame:performance.now()};
    }, true);
    const frame = now => { const m = window.modelMeasure; if(m) {m.maxFrameGap=Math.max(m.maxFrameGap,now-m.lastFrame);m.lastFrame=now;}requestAnimationFrame(frame); }; requestAnimationFrame(frame);
    pane.term.onRender(() => {
      const m = window.modelMeasure; if (!m || m.renderMs) return;
      const b = pane.term.buffer.active;
      const text = Array.from({length:b.length},(_,i)=>b.getLine(i).translateToString(true)).join(' ');
      if (text.includes('All available models') && text.includes('Esc close')) {
        m.renderMs = performance.now()-m.start;
        requestAnimationFrame(() => { if(window.modelMeasure===m) { modelSamples.push({renderMs:m.renderMs,nextFrameMs:performance.now()-m.start,maxFrameGap:m.maxFrameGap});window.modelMeasure=null; } });
      }
    });
  })()`);
  for(let i=0;i<10;i++) {
    await c.send('Input.insertText',{text:'/model'}); await delay(160);
    await c.key('Enter','Enter',13);
    await c.waitFor(`modelSamples.length === ${i+1}`);
    if(i===0) await c.screenshot('.tmp/xterm-model-tested.png');
    await c.key('Escape','Escape',27); await delay(180);
  }
  await c.send('Input.insertText',{text:'中文测试'});
  await c.waitFor(`(()=>{const b=[...agentdock.terminals.values()][0].term.buffer.active;return Array.from({length:b.length},(_,i)=>b.getLine(i).translateToString(true)).join(' ').includes('中文测试')})()`);
  await c.screenshot('.tmp/xterm-chinese.png');
  for(let i=0;i<4;i++) await c.key('Backspace','Backspace',8);
  const samples=await c.evaluate('modelSamples');
  const result={samples,chineseInsertText:true,note:'DOM keydown to xterm onRender and next requestAnimationFrame; not physical keyboard-to-pixel latency. Input.insertText tests committed Chinese text, not OS IME candidate UI.'};
  await writeFile('.tmp/xterm-model-results.json',JSON.stringify(result,null,2));
  console.log(JSON.stringify(result,null,2));
} finally {c.close();}
