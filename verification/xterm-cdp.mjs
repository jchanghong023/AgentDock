// Node 24+, no npm dependencies. Connect only to an explicitly enabled local test port.
import {writeFile} from 'node:fs/promises';
export const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
export async function connect(port) {
  let target;
  for (let i = 0; i < 100; i++) {
    try { target = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find(t => t.type === 'page' && t.url.includes('agentdock.localhost')); } catch {}
    if (target) break;
    await delay(100);
  }
  if (!target) throw Error('Embedded WebView2 target unavailable');
  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
  let next = 0; const pending = new Map();
  socket.onmessage = ({data}) => {
    const message = JSON.parse(data); const waiter = pending.get(message.id);
    if (waiter) { pending.delete(message.id); clearTimeout(waiter.timer); message.error ? waiter.reject(Error(JSON.stringify(message.error))) : waiter.resolve(message.result); }
  };
  function send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++next; const timer = setTimeout(() => { pending.delete(id); reject(Error(`CDP timeout: ${method}`)); }, 15000);
      pending.set(id, {resolve, reject, timer}); socket.send(JSON.stringify({id, method, params}));
    });
  }
  async function evaluate(expression) {
    const result = await send('Runtime.evaluate', {expression, returnByValue:true, awaitPromise:true});
    if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  }
  async function waitFor(expression, timeout = 15000) {
    const start = performance.now();
    while (performance.now() - start < timeout) { if (await evaluate(expression)) return performance.now() - start; await delay(20); }
    throw Error(`Condition timed out: ${expression}`);
  }
  async function key(key, code, virtualKey, modifiers = 0) {
    await send('Input.dispatchKeyEvent', {type:'keyDown', key, code, windowsVirtualKeyCode:virtualKey, modifiers});
    await send('Input.dispatchKeyEvent', {type:'keyUp', key, code, windowsVirtualKeyCode:virtualKey, modifiers});
  }
  return {send, evaluate, waitFor, key, close:() => socket.close(),
    text:() => evaluate(`(()=>{const t=[...agentdock.terminals.values()].find(p=>p.element.classList.contains('active'))?.term;if(!t)return '';return Array.from({length:t.buffer.active.length},(_,i)=>t.buffer.active.getLine(i).translateToString(true)).join(String.fromCharCode(10))})()`),
    screenshot:async path => { const {data} = await send('Page.captureScreenshot', {format:'png'}); await writeFile(path, Buffer.from(data,'base64')); },
  };
}
