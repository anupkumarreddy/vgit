import { describe, expect, it } from 'vitest';
import { classifyRef, layoutGraph } from './graph';
import type { Commit } from './types';

const commit = (hash: string, parents: string[] = []): Commit => ({
  hash, shortHash: hash.slice(0, 7), parents, author: 'A', email: 'a@example.test', date: '2026-01-01', subject: hash, refs: []
});

describe('layoutGraph', () => {
  it('creates diverging and converging edges from real parent hashes', () => {
    const rows = layoutGraph([
      commit('merge', ['main1', 'feature1']),
      commit('feature1', ['base']),
      commit('main1', ['base']),
      commit('base')
    ]);
    expect(rows[0].edges).toHaveLength(2);
    expect(rows[0].edges.some(edge => edge.merge)).toBe(true);
    expect(rows.some(row => row.columns >= 2)).toBe(true);
  });
});

describe('classifyRef', () => {
  it('distinguishes HEAD, tags, local branches, and remotes', () => {
    expect(classifyRef('HEAD -> main')).toEqual({ kind: 'head', label: 'main' });
    expect(classifyRef('tag: v1.0')).toEqual({ kind: 'tag', label: 'v1.0' });
    expect(classifyRef('origin/main').kind).toBe('remote');
    expect(classifyRef('feature').kind).toBe('branch');
  });
});
