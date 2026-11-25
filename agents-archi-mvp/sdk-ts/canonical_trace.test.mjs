import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {canonicalTraceJson} from './canonical_trace.mjs';

const hash = value => createHash('sha256').update('ARCHI_TRACE_V2\0').update(canonicalTraceJson(value)).digest('hex');

test('nested trace mutation changes commitment', () => {
    assert.notEqual(hash({inputs:{x:1},toolCalls:[]}), hash({inputs:{x:2},toolCalls:[]}));
    assert.notEqual(hash({toolCalls:[{result:{x:1}}]}), hash({toolCalls:[{result:{x:2}}]}));
});
test('key ordering does not affect commitment', () => {
    assert.equal(hash({inputs:{b:2,a:1}}), hash({inputs:{a:1,b:2}}));
});
test('lossy or cyclic JSON values are rejected', () => {
    for (const value of [{x:undefined}, {x:NaN}, [,,1], new Date(), {x:1n}]) {
        assert.throws(() => canonicalTraceJson(value), TypeError);
    }
    const cyclic = {};
    cyclic.self = cyclic;
    assert.throws(() => canonicalTraceJson(cyclic), TypeError);
});
