const { contextBridge, ipcRenderer } = require('electron');

const invoke = (channel, ...args) => ipcRenderer.invoke(channel, ...args);

contextBridge.exposeInMainWorld('vgit', {
  initialRepository: () => invoke('repo:initial'),
  chooseRepository: () => invoke('repo:choose'),
  openRepository: path => invoke('repo:open', path),
  refresh: () => invoke('repo:refresh'),
  diff: (path, staged) => invoke('repo:diff', path, staged),
  showCommit: hash => invoke('repo:showCommit', hash),
  stage: path => invoke('repo:stage', path),
  unstage: path => invoke('repo:unstage', path),
  stageAll: () => invoke('repo:stageAll'),
  unstageAll: () => invoke('repo:unstageAll'),
  discard: (path, untracked) => invoke('repo:discard', path, untracked),
  commit: (message, amend) => invoke('repo:commit', message, amend),
  checkout: branch => invoke('repo:checkout', branch),
  createBranch: branch => invoke('repo:createBranch', branch),
  fetch: () => invoke('repo:fetch'),
  pull: () => invoke('repo:pull'),
  push: () => invoke('repo:push'),
  stash: message => invoke('repo:stash', message),
  stashApply: index => invoke('repo:stashApply', index),
  reveal: () => invoke('repo:reveal')
});
