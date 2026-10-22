import test from 'node:test';import assert from 'node:assert/strict';
import {transportControl} from './tray-controls.mjs';
test('tray uses icon-only stop/resume and refuses resume without a known receiver',()=>{
  assert.deepEqual(transportControl({playing:true,busy:false,can_resume:false}),{glyph:'⏸',disabled:false});
  assert.deepEqual(transportControl({playing:false,busy:false,can_resume:true}),{glyph:'▶',disabled:false});
  assert.equal(transportControl({playing:false,busy:false,can_resume:false}).disabled,true);
});
test('tray cannot stack transport operations while a request is in flight',()=>{
  assert.equal(transportControl({playing:true,busy:true,can_resume:true}).disabled,true);
  assert.equal(transportControl({playing:false,busy:false,can_resume:true},true).disabled,true);
});
