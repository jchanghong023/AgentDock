/* The PTY is owned by Rust. xterm.js alone owns VT parsing and display state. */
(() => {
  'use strict';
  const terminals = new Map();
  const encoder = new TextEncoder();
  const INPUT_LIMIT = 1024 * 1024;
  let active = null;
  let resizeScheduled = false;
  function post(kind, pane, extra = {}) {
    window.ipc.postMessage(JSON.stringify({kind, id: pane?.id, epoch: pane?.epoch, ...extra}));
  }
  function notice(text) {
    const node = document.getElementById('notice');
    node.textContent = text; node.style.display = 'block';
    clearTimeout(notice.timer); notice.timer = setTimeout(() => node.style.display = 'none', 5000);
  }
  function base64(bytes) {
    let text = '';
    for (let i = 0; i < bytes.length; i += 8192) text += String.fromCharCode(...bytes.subarray(i, i + 8192));
    return btoa(text);
  }
  function decode(text) { return Uint8Array.from(atob(text), c => c.charCodeAt(0)); }
  function pump(pane) {
    if (pane.writing || !pane.queue.length) return;
    const bytes = pane.queue[0];
    const chunk = bytes.subarray(0, 32768);
    if (chunk.length === bytes.length) pane.queue.shift(); else pane.queue[0] = bytes.subarray(chunk.length);
    pane.writing = chunk.length;
    post('input', pane, {data: base64(chunk)});
  }
  function input(pane, bytes) {
    if (pane.pending + bytes.length > INPUT_LIMIT) { notice('输入队列已满或粘贴超过 1 MiB，请等待后重试'); return; }
    pane.pending += bytes.length; pane.queue.push(bytes); pump(pane);
  }
  function fit() {
    resizeScheduled = false;
    const pane = terminals.get(active);
    if (!pane || pane.element.clientWidth === 0 || pane.element.clientHeight === 0) return;
    const dimensions = pane.fit.proposeDimensions();
    if (dimensions) pane.term.resize(Math.max(2, Math.min(1000, dimensions.cols)), Math.max(1, Math.min(500, dimensions.rows)));
  }
  function scheduleFit() { if (!resizeScheduled) { resizeScheduled = true; requestAnimationFrame(fit); } }
  new ResizeObserver(scheduleFit).observe(document.body);
  function create(command) {
    const previous = terminals.get(command.id);
    if (previous?.epoch === command.epoch) return;
    if (previous) { previous.term.dispose(); previous.element.remove(); }
    const element = document.createElement('div'); element.className = 'terminal-pane'; element.dataset.session = command.id;
    document.body.appendChild(element);
    const term = new Terminal({
      fontFamily: command.font || 'Consolas, monospace', fontSize: command.size || 15,
      lineHeight: 1.4, scrollback: command.scrollback, cursorBlink: true,
      theme: {background:'#11151d', foreground:'#dce3ec', cursor:'#99d2ff', selectionBackground:'#334e6f'},
      allowProposedApi: true, allowTransparency: false, windowsPty: {backend:'conpty'},
    });
    const fitAddon = new FitAddon.FitAddon(); term.loadAddon(fitAddon);
    term.loadAddon(new Unicode11Addon.Unicode11Addon()); term.unicode.activeVersion = '11';
    const pane = {id:command.id, epoch:command.epoch, term, fit:fitAddon, element, queue:[], pending:0, writing:0};
    terminals.set(pane.id, pane);
    term.onData(data => input(pane, encoder.encode(data)));
    term.onBinary(data => input(pane, Uint8Array.from(data, c => c.charCodeAt(0))));
    term.onResize(({cols, rows}) => post('resize', pane, {cols, rows}));
    term.onTitleChange(title => {
      pane.title = title;
      if (!pane.titleScheduled) { pane.titleScheduled = true; queueMicrotask(() => { pane.titleScheduled = false; post('title', pane, {title:pane.title.slice(0,120)}); }); }
    });
    term.attachCustomKeyEventHandler(event => {
      if (event.type !== 'keydown') return true;
      const key = event.key.toLowerCase();
      if (event.ctrlKey && event.shiftKey && key === 'c') { post('copy', pane, {data:term.getSelection()}); return false; }
      if ((event.ctrlKey && event.shiftKey && key === 'v') || (event.shiftKey && key === 'insert')) { post('paste', pane); return false; }
      if (event.ctrlKey && ['=','+','-'].includes(key)) { post('zoom', pane, {data:key === '-' ? '-1' : '1'}); return false; }
      return true;
    });
    term.open(element);
  }
  window.agentdock = {
    terminals,
    receive(commands) {
      for (const command of commands) {
        try {
          if (command.kind === 'create') { create(command); continue; }
          if (command.kind === 'activate') {
            active = command.id;
            for (const pane of terminals.values()) {
              pane.element.classList.toggle('active', pane.id === active);
              pane.term.options.fontFamily = command.font; pane.term.options.fontSize = command.size;
            }
            fit(); terminals.get(active)?.term.focus(); continue;
          }
          const pane = terminals.get(command.id);
          if (!pane || (command.epoch && pane.epoch !== command.epoch)) continue;
          switch (command.kind) {
            case 'output':
              pane.term.write(decode(command.data), () => post('ack', pane));
              break;
            case 'written':
              pane.pending -= pane.writing; pane.writing = 0; pump(pane);
              break;
            case 'paste':
              if (encoder.encode(command.data).length <= INPUT_LIMIT) pane.term.paste(command.data);
              else notice('单次粘贴上限 1 MiB');
              break;
            case 'error': notice(command.data); break;
            case 'dispose': pane.term.dispose(); pane.element.remove(); terminals.delete(pane.id); break;
          }
        } catch (error) { notice(String(error)); post('error', null, {data:String(error)}); }
      }
    },
  };
  post('ready');
})();
