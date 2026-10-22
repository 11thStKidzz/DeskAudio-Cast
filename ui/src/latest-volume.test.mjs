import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile(new URL('./latest-volume.ts', import.meta.url), 'utf8');
const code = ts.transpileModule(source, {compilerOptions: {module: ts.ModuleKind.ESNext}}).outputText;
const {LatestVolume} = await import('data:text/javascript;base64,' + Buffer.from(code).toString('base64'));
const turn = () => new Promise(resolve => setImmediate(resolve));
test('fast dragging keeps at most one volume request and only the newest pending value', async () => {
  const sent = [], release = [];
  const changes = new LatestVolume(value => { sent.push(value); return new Promise(resolve => release.push(resolve)); }, assert.fail);
  changes.set(.2);
  for(let n=21;n<=90;n++) changes.set(n/100);
  assert.deepEqual(sent, [.2]);
  release.shift()(); await turn(); assert.deepEqual(sent, [.2,.9]);
  release.shift()(); await turn(); changes.set(.4); assert.deepEqual(sent, [.2,.9,.4]);
  release.shift()(); await turn();
});
test('a failed volume request does not block the latest value or leak rejections', async () => {
  let rejectFirst; const sent=[], errors=[];
  const changes = new LatestVolume(value => {
    sent.push(value); return sent.length === 1 ? new Promise((_,reject)=>rejectFirst=reject) : Promise.resolve();
  }, error => errors.push(error));
  changes.set(.1); changes.set(.3); rejectFirst('offline'); await turn();
  assert.deepEqual(sent,[.1,.3]); assert.deepEqual(errors,['offline']);
});
test('single volume, no endpoint-mute control, and native diagnostic/startup wiring', async () => {
  const html = await readFile(new URL('../index.html', import.meta.url),'utf8');
  const main = await readFile(new URL('./main.ts', import.meta.url),'utf8');
  assert.equal([...html.matchAll(/type="range"/g)].length,1);
  for(const removed of ['mute-local','receiver-volumes','auto-reconnect']) assert.equal(html.includes('id="'+removed+'"'),false);
  for(const present of ['launch-at-login','auto-connect','diagnostics-folder']) assert.ok(html.includes('id="'+present+'"'));
  assert.ok(main.includes('"export_audio_diagnostics"')); assert.equal(main.includes('createObjectURL'),false);
  assert.equal(main.includes('async function legacyBootstrap'),false);
  assert.equal(main.includes('"get_receiver_volumes"'),false);
});
