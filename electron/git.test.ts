import { describe, expect, it } from 'vitest';
import { parseLog, parseStatus } from './git.js';

describe('parseStatus', () => {
  it('parses tracked, renamed, untracked, and conflicted paths including spaces', () => {
    const raw = [
      '# branch.head main',
      '1 M. N... 100644 100644 100644 aaa bbb src/file one.ts',
      '2 R. N... 100644 100644 100644 aaa bbb R100 new name.ts',
      'old name.ts',
      '? untracked file.md',
      'u UU N... 100644 100644 100644 100644 aaa bbb ccc conflict.ts',
      ''
    ].join('\0');
    expect(parseStatus(raw)).toEqual([
      { path: 'src/file one.ts', staged: 'M', working: '.', conflicted: false },
      { path: 'new name.ts', originalPath: 'old name.ts', staged: 'R', working: '.', conflicted: false },
      { path: 'untracked file.md', staged: '?', working: '?', conflicted: false },
      { path: 'conflict.ts', staged: 'U', working: 'U', conflicted: true }
    ]);
  });
});

describe('parseLog', () => {
  it('parses commit fields and refs', () => {
    const raw = '\x1eabc\x1fabc1234\x1fparent1 parent2\x1fAda Lovelace\x1fada@example.com\x1f2026-01-01T10:00:00Z\x1fMerge feature\x1fHEAD -> main, tag: v1';
    expect(parseLog(raw)[0]).toMatchObject({
      hash: 'abc', parents: ['parent1', 'parent2'], author: 'Ada Lovelace', refs: ['HEAD -> main', 'tag: v1']
    });
  });
});
