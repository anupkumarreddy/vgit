import type { Commit } from './types';

export type GraphEdge = { from: number; to: number; color: string; merge: boolean };
export type GraphLayoutRow = { lane: number; columns: number; color: string; edges: GraphEdge[] };

const PALETTE = ['#a78bfa', '#34d399', '#60a5fa', '#fb923c', '#f472b6', '#22d3ee', '#facc15'];

function colorFor(value: string) {
  let hash = 0;
  for (let index = 0; index < value.length; index += 1) hash = ((hash << 5) - hash + value.charCodeAt(index)) | 0;
  return PALETTE[Math.abs(hash) % PALETTE.length];
}

export function layoutGraph(commits: Commit[]): GraphLayoutRow[] {
  let lanes: string[] = [];
  return commits.map(commit => {
    let lane = lanes.indexOf(commit.hash);
    if (lane === -1) {
      lane = lanes.length;
      lanes.push(commit.hash);
    }

    const before = [...lanes];
    const next = [...lanes];
    if (commit.parents.length) next[lane] = commit.parents[0];
    else next.splice(lane, 1);

    commit.parents.slice(1).forEach((parent, offset) => {
      if (!next.includes(parent)) next.splice(lane + 1 + offset, 0, parent);
    });

    // A parent already visible in another lane should converge into that lane.
    const unique = next.filter((hash, index) => next.indexOf(hash) === index);
    const edges: GraphEdge[] = [];
    before.forEach((hash, from) => {
      const targets = from === lane ? commit.parents : [hash];
      targets.forEach((target, parentIndex) => {
        const to = unique.indexOf(target);
        if (to >= 0) edges.push({ from, to, color: colorFor(target), merge: parentIndex > 0 });
      });
    });

    const row = {
      lane,
      columns: Math.max(before.length, unique.length, 1),
      color: colorFor(commit.hash),
      edges
    };
    lanes = unique;
    return row;
  });
}

export function classifyRef(ref: string) {
  const clean = ref.trim();
  if (clean.startsWith('HEAD -> ')) return { kind: 'head' as const, label: clean.slice(8) };
  if (clean.startsWith('tag: ')) return { kind: 'tag' as const, label: clean.slice(5) };
  if (clean.includes('/')) return { kind: 'remote' as const, label: clean };
  return { kind: 'branch' as const, label: clean };
}
