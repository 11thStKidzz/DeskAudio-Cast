import assert from 'node:assert/strict';
import test from 'node:test';
import {readFile} from 'node:fs/promises';
const read=path=>readFile(new URL(path,import.meta.url),'utf8');
const html=await read('../index.html');
const main=await read('./main.ts');
const config=JSON.parse(await read('../../src-tauri/tauri.conf.json'));
const capability=JSON.parse(await read('../../src-tauri/capabilities/default.json'));
test('production CSP blocks remote resources while allowing local Tauri IPC',()=>{
  assert.ok(config.app.security.csp.includes("default-src 'self'"));
  assert.ok(config.app.security.csp.includes('connect-src ipc: http://ipc.localhost'));
  assert.ok(config.app.security.csp.includes("object-src 'none'"));
  assert.ok(!config.app.security.csp.includes('unsafe-eval'));
  assert.ok(!config.app.security.csp.includes('https:'));
  assert.ok(!capability.permissions.some(p=>p.startsWith('shell:') || p.includes('store:')));
  assert.deepEqual(capability.permissions,['core:event:allow-listen','core:event:allow-unlisten']);
});
test('legal documents use local fixed commands and safe text rendering',()=>{
  for(const key of ['about','privacy','licenses','third_party','support']) assert.ok(html.includes(`data-legal="${key}"`));
  assert.ok(main.includes('get_legal_document'));
  assert.ok(main.includes('getElementById("legal-content")!.textContent = text'));
  assert.ok(!main.includes('open_latest_release'));
});
test('clearing saved device data requires confirmation and does not delete old logs',()=>{
  assert.ok(main.includes('askConfirmation(t("clear_device_data")'));
  assert.ok(main.includes('clear_saved_device_data'));
  assert.ok(html.includes('privacy_clear_hint'));
});
test('application, UI, lockfile and Tauri versions agree',async()=>{
  const pkg=JSON.parse(await read('../package.json'));
  const lock=JSON.parse(await read('../package-lock.json'));
  const cargo=await read('../../Cargo.toml');
  assert.equal(config.version,pkg.version);
  assert.equal(lock.version,pkg.version);
  assert.equal(lock.packages[''].version,pkg.version);
  assert.ok(cargo.includes(`version = "${pkg.version}"`));
});
