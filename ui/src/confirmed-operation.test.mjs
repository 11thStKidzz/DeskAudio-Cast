import test from 'node:test';
import assert from 'node:assert/strict';
import {confirmedOperation} from './confirmed-operation.mjs';
test('cancel does not invoke the backend or change saved information',async()=>{
  let changed=false;
  assert.equal(await confirmedOperation(async()=>false,async()=>{changed=true}),false);
  assert.equal(changed,false);
});
test('confirmation applies once, and failures are not reported as success',async()=>{
  let calls=0;
  assert.equal(await confirmedOperation(async()=>true,async()=>{calls++}),true);
  assert.equal(calls,1);
  await assert.rejects(confirmedOperation(async()=>true,async()=>{throw new Error('fixture')}));
});
