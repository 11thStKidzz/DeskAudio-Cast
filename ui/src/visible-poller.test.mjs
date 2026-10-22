import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile(new URL('./visible-poller.ts', import.meta.url), 'utf8');
const js = ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2021}}).outputText;
const { VisiblePoller } = await import(`data:text/javascript;base64,${Buffer.from(js).toString('base64')}`);
const flush = async () => { for(let i=0;i<16;i++) await Promise.resolve(); };

async function clockTest(body) {
  const originalSet=globalThis.setTimeout, originalClear=globalThis.clearTimeout;
  let now=0, id=0;
  const timers=new Map();
  globalThis.setTimeout=(fn,ms)=>{const key=++id;timers.set(key,{at:now+ms,fn});return key;};
  globalThis.clearTimeout=key=>timers.delete(key);
  const advance=async ms=>{
    const end=now+ms;
    for(let guard=0;guard<10000;guard++){
      const entry=[...timers].sort((a,b)=>a[1].at-b[1].at)[0];
      if(!entry||entry[1].at>end){now=end;await flush();return;}
      now=entry[1].at;timers.delete(entry[0]);entry[1].fn();await flush();
    }
    throw new Error('unbounded timer loop');
  };
  try { await body({timers,advance}); }
  finally {globalThis.setTimeout=originalSet;globalThis.clearTimeout=originalClear;}
}

test('100 hide/show cycles leave no hidden polling or timer accumulation',()=>clockTest(async({timers,advance})=>{
  let calls=0,renders=0;
  const poller=new VisiblePoller(async()=>++calls,()=>renders++,10,100);
  for(let i=0;i<100;i++){
    poller.setVisible(true);await flush();
    assert.equal(timers.size,1);
    poller.setVisible(false);assert.equal(timers.size,0);
    await advance(1000);
  }
  assert.equal(calls,100);assert.equal(renders,100);
  poller.dispose();assert.equal(timers.size,0);
}));

test('a hidden or obsolete response never renders into the reopened window',()=>clockTest(async({advance})=>{
  let resolve,calls=0;
  const values=[];
  const pending=new Promise(r=>resolve=r);
  const poller=new VisiblePoller(()=>++calls===1?pending:Promise.resolve('fresh'),value=>values.push(value),10,100);
  poller.setVisible(true);await flush();
  poller.setVisible(false);poller.setVisible(true);
  resolve('obsolete');await flush();
  assert.deepEqual(values,[]);
  await advance(10);assert.deepEqual(values,['fresh']);
  poller.dispose();
}));

test('hung native requests do not accumulate after timeouts or repeated reopening',()=>clockTest(async({advance,timers})=>{
  let resolve,calls=0;
  const values=[];
  const pending=new Promise(r=>resolve=r);
  const poller=new VisiblePoller(()=>++calls===1?pending:Promise.resolve('recovered'),value=>values.push(value),10,100);
  poller.setVisible(true);await flush();await advance(100);
  for(let i=0;i<100;i++){poller.setVisible(false);poller.setVisible(true);}
  await advance(3600000);
  assert.equal(calls,1);assert.equal(timers.size,0);assert.deepEqual(values,[]);
  resolve('late');await flush();await advance(10);
  assert.deepEqual(values,['recovered']);
  poller.dispose();
}));

test('request errors recover and disposed pollers never restart',()=>clockTest(async({advance,timers})=>{
  let calls=0,renders=0;
  const poller=new VisiblePoller(async()=>{if(++calls===1)throw new Error('test');return calls;},()=>renders++,10,100);
  poller.setVisible(true);await flush();await advance(10);
  assert.equal(calls,2);assert.equal(renders,1);
  poller.dispose();poller.setVisible(true);await advance(1000);
  assert.equal(calls,2);assert.equal(timers.size,0);
}));
