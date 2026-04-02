/**
 * Basic tests for mast-memory Node.js bindings.
 * These tests require the native module to be built first via `napi build --release`.
 */

// Note: Tests are skipped if the native module isn't built
let MastDb: any;
try {
  ({ MastDb } = require('../index'));
} catch {
  // Module not built yet
}

import * as os from 'os';
import * as path from 'path';
import * as fs from 'fs';

function tmpDbPath(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'mast-test-'));
  return path.join(dir, 'test.mast');
}

function dummyEmbed(text: string): number[] {
  let h = 0;
  for (let i = 0; i < text.length; i++) {
    h = ((h << 5) - h + text.charCodeAt(i)) | 0;
  }
  h = Math.abs(h);
  const vec: number[] = [];
  for (let i = 0; i < 32; i++) {
    h = (h * 1103515245 + 12345) & 0x7FFFFFFF;
    vec.push((h / 0x7FFFFFFF) * 2 - 1);
  }
  const norm = Math.sqrt(vec.reduce((s, x) => s + x * x, 0));
  return norm > 0 ? vec.map(x => x / norm) : vec;
}

const describeIfBuilt = MastDb ? describe : describe.skip;

describeIfBuilt('MastDb', () => {
  test('store and recall', () => {
    const db = new MastDb(tmpDbPath());
    const mem = db.store('test', 'hello world', dummyEmbed, 32);
    expect(mem.id).toBeTruthy();
    expect(mem.content).toBe('hello world');

    const results = db.recall('test', dummyEmbed, 32, 'hello');
    expect(results.length).toBeGreaterThan(0);
    expect(results[0].memory.content).toBe('hello world');
    db.close();
  });

  test('get and delete', () => {
    const db = new MastDb(tmpDbPath());
    const mem = db.store('test', 'to delete', dummyEmbed, 32);

    const found = db.get('test', mem.id);
    expect(found).not.toBeNull();
    expect(found!.content).toBe('to delete');

    db.delete('test', mem.id);
    expect(db.get('test', mem.id)).toBeNull();
    db.close();
  });

  test('info', () => {
    const db = new MastDb(tmpDbPath());
    db.store('col_a', 'first', dummyEmbed, 32);
    db.store('col_a', 'second', dummyEmbed, 32);

    const info = db.info();
    expect(info.length).toBe(1);
    expect(info[0].name).toBe('col_a');
    expect(info[0].memoryCount).toBe(2);
    db.close();
  });

  test('relate and traverse', () => {
    const db = new MastDb(tmpDbPath());
    const edgeId = db.relate('test', 'alice', 'bob', 'knows', 0.9);
    expect(edgeId).toBeTruthy();

    const edges = db.traverse('test', 'alice');
    expect(edges.length).toBe(1);
    expect(edges[0].target).toBe('bob');
    db.close();
  });

  test('snapshot and restore', () => {
    const dbPath1 = tmpDbPath();
    const db1 = new MastDb(dbPath1);
    db1.store('snap', 'memory 1', dummyEmbed, 32);
    db1.store('snap', 'memory 2', dummyEmbed, 32);

    const snapPath = dbPath1 + '.snapshot.jsonl';
    const snapResult = db1.snapshot('snap', snapPath);
    expect(snapResult.memoryCount).toBe(2);
    db1.close();

    const dbPath2 = tmpDbPath();
    const db2 = new MastDb(dbPath2);
    const restoreResult = db2.restore(snapPath);
    expect(restoreResult.memoryCount).toBe(2);

    const memories = db2.list('snap');
    expect(memories.length).toBe(2);
    db2.close();
  });
});
