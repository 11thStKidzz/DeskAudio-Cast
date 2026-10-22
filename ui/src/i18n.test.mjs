import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

const compile = source => ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2021 } }).outputText;
const url = source => `data:text/javascript;base64,${Buffer.from(source).toString('base64')}`;
const localesUrl = url(compile(await readFile(new URL('./locales.ts', import.meta.url), 'utf8')));
const source = await readFile(new URL('./i18n.ts', import.meta.url), 'utf8');
const html = await readFile(new URL('../index.html', import.meta.url), 'utf8');
const main = await readFile(new URL('./main.ts', import.meta.url), 'utf8');
const store = new Map();
globalThis.localStorage = { getItem: k => store.get(k) ?? null, setItem: (k,v) => store.set(k,v) };
globalThis.document = { documentElement: {lang:''}, querySelectorAll: () => [] };
const { DICTS, languages, t, setLang, getLang, onLangChange, applyStaticTranslations } =
  await import(url(compile(source).replace('"./locales"', JSON.stringify(localesUrl))));
const keys = new Set([
  ...[...html.matchAll(/data-i18n="([^"]+)"/g)].map(m=>m[1]),
  ...[...html.matchAll(/data-i18n-attr="([^"]+)"/g)].flatMap(m=>m[1].split(',').map(p=>p.split(':')[1])),
  ...[...main.matchAll(/\bt\("([^"]+)"/g)].map(m=>m[1]),
  ...['connection_pairing','connection_ready','connection_failed','connection_streaming',
      'mode_ultra_low_hint','mode_balanced_hint','mode_stable_hint', 'devices_count_one',
      'devices_count_other', 'latency_current', 'group_first','group_second',
      'mute_off','mute_waiting_signal','mute_checking','mute_muted','mute_unsupported','mute_user_changed',
      'feedback_failed','transport_failed','receiver_unavailable','session_cancelled'],
]);

test('Chinese first, all six dictionaries cover every active UI key', () => {
  assert.equal(getLang(), 'zh');
  assert.equal(languages.length, 6);
  for (const lang of languages) for (const key of keys) {
    assert.ok(DICTS[lang][key]?.trim(), `${lang}: missing ${key}`);
    const expected = [...new Set([...(DICTS.en[key] ?? '').matchAll(/\{([^}]+)\}/g)].map(m=>m[1]))].sort();
    const actual = [...new Set([...DICTS[lang][key].matchAll(/\{([^}]+)\}/g)].map(m=>m[1]))].sort();
    assert.deepEqual(actual, expected, `${lang}: placeholders ${key}`);
  }
});

test('language switching persists, updates document and calls subscribers', () => {
  let changes = 0;
  const unsubscribe = onLangChange(()=>changes++);
  for (const lang of ['en','es','ru','de','ja','zh']) {
    setLang(lang);
    assert.equal(getLang(),lang);
    assert.equal(document.documentElement.lang,lang);
    assert.equal(store.get('desktop-audio-sender.lang'),lang);
    assert.equal(t('play_to',{name:'Kitchen'}).includes('{name}'),false);
    assert.ok(t('play_to',{name:'Kitchen'}).includes('Kitchen'));
  }
  setLang('unsupported');
  assert.equal(getLang(),'zh');
  assert.equal(changes,6);
  unsubscribe();
});

test('static labels and accessibility attributes use current language', () => {
  const label = {dataset:{i18n:'scan'},textContent:''};
  const attrs = {};
  const control = {dataset:{i18nAttr:'title:lang_toggle_title,aria-label:playback_mode'},setAttribute:(k,v)=>attrs[k]=v};
  document.querySelectorAll = selector => selector==='[data-i18n]' ? [label] : [control];
  applyStaticTranslations();
  assert.equal(label.textContent,'搜索设备');
  assert.equal(attrs['aria-label'],'播放模式');
  assert.equal(attrs.title,'切换界面语言');
  document.querySelectorAll = () => [];
});

test('slider and legacy checkbox removed; three buttons are accessible', () => {
  assert.equal(html.includes('id="latency"'), false);
  assert.equal(html.includes('id="experimental-local-buffer"'), false);
  for (const mode of ['ultra_low','balanced','stable']) assert.ok(html.includes(`id="mode-${mode}"`));
  assert.ok(html.includes('aria-pressed="true"'));
  assert.ok(main.includes('playbackMode: mode'));
  assert.ok(main.includes('"get_ui_language"'));
  assert.ok(main.includes('"save_ui_language"'));
  assert.ok(main.includes('playerStatus.dataset.i18n = key'));
});
