// Real ConPTY fixture. Reads only test data; never evaluates commands.
import {createHash} from 'node:crypto';
process.stdin.setRawMode(true);process.stdin.setEncoding('utf8');
let input='';
process.stdout.write('FIXTURE_READY\r\n');
process.stdout.on('resize',()=>process.stdout.write(`SIZE:${process.stdout.columns}x${process.stdout.rows}\r\n`));
process.stdin.on('data',data=>{
  input+=data;
  for (;;) {
    const end=input.indexOf('\r');if(end<0)break;
    const line=input.slice(0,end);input=input.slice(end+1);
    if(line==='bulk') {
      process.stdout.write('\x1b[2J\x1b[H');
      process.stdout.write(Array.from({length:10000},(_,i)=>`ROW:${String(i).padStart(5,'0')}:中文你好:abcdefghijklmnopqrstuvwxyz0123456789\r\n`).join('')+'BULK_DONE\r\n');
    } else if(line==='alternate') {
      process.stdout.write('\x1b[?1049h\x1b[2J\x1b[H\x1b[32mALTERNATE_中文\x1b[0m');
    } else if(line==='normal') {
      process.stdout.write('\x1b[?1049lNORMAL_RESTORED\r\n');
    } else if(line.startsWith('hash:')) {
      process.stdout.write(`HASH:${createHash('sha256').update(line.slice(5)).digest('hex')}\r\n`);
    } else if(line==='quit') {process.exit(0);}
    else process.stdout.write(`ECHO:${line}\r\n`);
  }
});
